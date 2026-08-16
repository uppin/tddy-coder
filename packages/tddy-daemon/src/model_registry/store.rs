//! The per-daemon registry's SQLite store: providers, their cached model catalogs, and the
//! assistants composed from those models.
//!
//! Follows the `session_catalog` precedent (`tddy-core/src/session_catalog/store.rs`): `sqlx` with
//! the runtime query API only (no `query!` macro, so no compile-time database), WAL journal,
//! `Normal` synchronous, a 5 s busy timeout, and the file created on demand.
//!
//! Two rules run through the whole file:
//!
//! - **The database holds plaintext api keys**, so it is `0600` from the moment it exists, and so
//!   are the `-wal`/`-shm` files SQLite writes the same rows through.
//! - **Everyone reads, the owner writes.** Every row records the operator who created it. Listing
//!   is fleet-wide (the screen is an overview); updating, deleting and reading a credential are
//!   the owner's alone. A row written before the registry had owners carries `NULL`, which means
//!   "unowned" and is writable by anyone — see the changeset for why that beats the alternatives.

use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use tddy_discovery::agent_def::{builtin_agent_defs, SubagentTool};
use tddy_service::proto::models::{AssistantEntry, ModelEntry, ProviderEntry, ProviderKind};

use super::error::{truncate_provider_detail, ModelRegistryError};

/// Owner-only permissions: this database holds live provider credentials.
#[cfg(unix)]
const OWNER_ONLY_FILE: u32 = 0o600;

/// The files SQLite writes the registry through, relative to the database path. A credential
/// written in WAL mode lives in `-wal` before it lives in the database itself, so all three carry
/// the same mode.
#[cfg(unix)]
const SIBLING_SUFFIXES: [&str; 2] = ["-wal", "-shm"];

/// The URL schemes a provider may be configured on.
///
/// An allowlist rather than a denylist: an authenticated caller supplies this URL and the daemon
/// then fetches it and echoes what came back, so anything outside "an HTTP endpoint" (`file:`,
/// `gopher:`, a bare `localhost:11434`) must be refused rather than attempted.
const ALLOWED_BASE_URL_SCHEMES: [&str; 2] = ["http", "https"];

/// The `BEGIN` every check-then-act sequence here uses.
///
/// SQLite's default `BEGIN` is deferred: the transaction takes its read lock at the first `SELECT`
/// and only tries to upgrade at the first write, so two callers can both pass the same "is this
/// name free?" check. `IMMEDIATE` takes the write lock up front, which serializes the whole
/// check-and-insert — cheap here, since a registry write happens when an operator clicks a button.
const BEGIN_WRITE: &str = "BEGIN IMMEDIATE";

/// A provider as the caller asks for it to be created. Distinct from [`ProviderEntry`] because the
/// api key travels *in* and never back out.
#[derive(Debug, Clone)]
pub struct NewProvider {
    pub kind: ProviderKind,
    pub label: String,
    pub base_url: String,
    /// Stored on this daemon; no read path ever returns it (see [`ModelRegistryStore::credential_for`]).
    pub api_key: Option<String>,
}

/// An assistant as the caller asks for it to be created.
#[derive(Debug, Clone)]
pub struct NewAssistant {
    /// The `--agent` value. Unique per daemon, and never a builtin agent's name.
    pub name: String,
    pub label: String,
    pub provider_id: String,
    pub model_id: String,
    pub system_prompt: String,
    /// Exec-catalog tool names. Every entry must resolve via [`SubagentTool::from_catalog_name`].
    pub tools: Vec<String>,
}

/// The registry of one daemon, backed by a SQLite file.
pub struct ModelRegistryStore {
    pool: SqlitePool,
    /// Stamped onto every row this store hands out, so the web can tell whose registry a merged
    /// row came from after fanning out across the common room.
    daemon_instance_id: String,
    /// Agent ids an assistant may not take, beyond the builtin defs and the coding backends:
    /// this daemon's `allowed_agents` config entries. Supplied at open time
    /// ([`ModelRegistryStore::reserving_agent_ids`]) because the store cannot read daemon config.
    reserved_agent_ids: Vec<String>,
}

impl ModelRegistryStore {
    /// Open (creating if missing) the registry at `db_path`, ensuring its schema exists and that
    /// the operator running this daemon is the only account that can read it.
    pub async fn open(
        db_path: &Path,
        daemon_instance_id: &str,
    ) -> Result<Self, ModelRegistryError> {
        if let Some(parent) = db_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    io_failure(&format!("model registry directory {}", parent.display()), e)
                })?;
            }
        }
        // Created owner-only *before* SQLite opens it, so a key is never briefly world-readable,
        // and so the `-wal`/`-shm` files SQLite derives from the database's own mode start there
        // too. An existing database keeps its contents and is re-restricted below, which is what
        // repairs one an earlier daemon left at 0644.
        precreate_owner_only(db_path)?;

        let options = SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            // Stated rather than inherited: the `model` and `assistant` rows are meaningless
            // without the provider they name, and a default is not a guarantee.
            .foreign_keys(true)
            .busy_timeout(std::time::Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        ensure_schema(&pool).await?;
        restrict_to_owner(db_path)?;
        Ok(Self {
            pool,
            daemon_instance_id: daemon_instance_id.to_string(),
            reserved_agent_ids: Vec::new(),
        })
    }

    /// Also refuse assistant names colliding with these agent ids — the daemon's `allowed_agents`
    /// config entries. `--agent <id>` resolves one name space, so an assistant that shadows a
    /// configured coding backend would make which one starts depend on resolution order.
    pub fn reserving_agent_ids(mut self, ids: impl IntoIterator<Item = String>) -> Self {
        self.reserved_agent_ids = ids.into_iter().collect();
        self
    }

    // --- Providers ---------------------------------------------------------

    /// Add a provider owned by `owner`. Its base URL identifies it: a second provider on the same
    /// endpoint would give the same model two registry rows, so it is refused rather than
    /// deduplicated later.
    ///
    /// The duplicate check, the id mint and the insert share one write transaction: separately,
    /// two operators adding the same endpoint at once would both see it free.
    pub async fn create_provider(
        &self,
        provider: NewProvider,
        owner: &str,
    ) -> Result<ProviderEntry, ModelRegistryError> {
        validate_base_url(&provider.base_url)?;

        let mut tx = self.pool.begin_with(BEGIN_WRITE).await?;
        let existing: Option<String> =
            sqlx::query_scalar("SELECT provider_id FROM provider WHERE base_url = ?1")
                .bind(&provider.base_url)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some(provider_id) = existing {
            return Err(ModelRegistryError::AlreadyExists(format!(
                "a provider for {} is already configured ({provider_id})",
                provider.base_url
            )));
        }

        let provider_id = mint_provider_id(&mut tx, provider.kind).await?;
        sqlx::query(
            "INSERT INTO provider
                (provider_id, kind, label, base_url, credential, credential_ref, enumeration_error,
                 owner)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, '', ?6)",
        )
        .bind(&provider_id)
        .bind(provider.kind as i32)
        .bind(&provider.label)
        .bind(&provider.base_url)
        .bind(provider.api_key.as_deref())
        .bind(owner)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            already_exists_on_unique_violation(
                e,
                &format!("a provider for {} is already configured", provider.base_url),
            )
        })?;
        tx.commit().await?;

        Ok(ProviderEntry {
            provider_id,
            kind: provider.kind as i32,
            label: provider.label,
            base_url: provider.base_url,
            has_credential: provider.api_key.is_some(),
            daemon_instance_id: self.daemon_instance_id.clone(),
            enumeration_error: String::new(),
        })
    }

    /// Every provider on this daemon, in creation order — every operator's, since the screen is a
    /// fleet-wide overview. Never carries a credential, only the `has_credential` flag, so a
    /// response cannot leak a key by accident.
    pub async fn list_providers(&self) -> Result<Vec<ProviderEntry>, ModelRegistryError> {
        let rows = sqlx::query(
            "SELECT provider_id, kind, label, base_url, credential IS NOT NULL AS has_credential,
                    enumeration_error
             FROM provider
             ORDER BY rowid",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|row| ProviderEntry {
                provider_id: row.get("provider_id"),
                kind: row.get("kind"),
                label: row.get("label"),
                base_url: row.get("base_url"),
                has_credential: row.get::<i64, _>("has_credential") != 0,
                daemon_instance_id: self.daemon_instance_id.clone(),
                enumeration_error: row.get("enumeration_error"),
            })
            .collect())
    }

    /// One provider by id.
    pub async fn provider(&self, provider_id: &str) -> Result<ProviderEntry, ModelRegistryError> {
        self.list_providers()
            .await?
            .into_iter()
            .find(|p| p.provider_id == provider_id)
            .ok_or_else(|| {
                ModelRegistryError::NotFound(format!("no provider {provider_id} on this daemon"))
            })
    }

    /// The stored api key for `provider_id`, for the one caller that needs it: the provider client
    /// about to authenticate against that endpoint — and only when `caller` owns the row.
    ///
    /// The refusal does not depend on whether a key is actually stored. If it did, a colleague's
    /// refresh or chat would work against a keyless endpoint today and start failing the day its
    /// owner added a key; and a caller told "no credential" would go on to talk to someone else's
    /// endpoint unauthenticated, which is exactly the silent behavior this registry must not have.
    pub async fn credential_for(
        &self,
        provider_id: &str,
        caller: &str,
    ) -> Result<Option<String>, ModelRegistryError> {
        let row = sqlx::query("SELECT credential, owner FROM provider WHERE provider_id = ?1")
            .bind(provider_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| {
                ModelRegistryError::NotFound(format!("no provider {provider_id} on this daemon"))
            })?;
        authorize(
            row.get::<Option<String>, _>("owner"),
            caller,
            &format!("provider {provider_id}"),
        )?;
        Ok(row.get::<Option<String>, _>("credential"))
    }

    /// Remove a provider `caller` owns, and the models cached for it. An assistant still built on
    /// it is a refusal, not a cascade — deleting the provider would leave that assistant pointing
    /// at an endpoint that no longer exists.
    ///
    /// The dependents check runs in the same write transaction as the delete, so an assistant
    /// created between the two cannot survive its provider.
    pub async fn delete_provider(
        &self,
        provider_id: &str,
        caller: &str,
    ) -> Result<(), ModelRegistryError> {
        let mut tx = self.pool.begin_with(BEGIN_WRITE).await?;
        let owner: Option<String> =
            sqlx::query("SELECT owner FROM provider WHERE provider_id = ?1")
                .bind(provider_id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(|| {
                    ModelRegistryError::NotFound(format!(
                        "no provider {provider_id} on this daemon"
                    ))
                })?
                .get("owner");
        authorize(owner, caller, &format!("provider {provider_id}"))?;

        let dependents: Vec<String> =
            sqlx::query_scalar("SELECT name FROM assistant WHERE provider_id = ?1 ORDER BY rowid")
                .bind(provider_id)
                .fetch_all(&mut *tx)
                .await?;
        if !dependents.is_empty() {
            return Err(ModelRegistryError::InUse(format!(
                "provider {provider_id} is still used by assistant(s): {}",
                dependents.join(", ")
            )));
        }

        sqlx::query("DELETE FROM model WHERE provider_id = ?1")
            .bind(provider_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM provider WHERE provider_id = ?1")
            .bind(provider_id)
            .execute(&mut *tx)
            .await?;
        // The id is retired rather than freed. It keys model rows, assistant rows, log lines and
        // every per-row action in the UI; handing `prov-ollama` to the next Ollama someone adds
        // would make an in-flight refresh of the old provider land in the new one's catalog.
        sqlx::query("INSERT OR IGNORE INTO retired_provider_id (provider_id) VALUES (?1)")
            .bind(provider_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Record why the last enumeration of `provider_id` failed, so the screen can show the cause
    /// next to the provider instead of an unexplained empty model list.
    pub async fn record_enumeration_error(
        &self,
        provider_id: &str,
        message: &str,
    ) -> Result<(), ModelRegistryError> {
        let updated =
            sqlx::query("UPDATE provider SET enumeration_error = ?2 WHERE provider_id = ?1")
                .bind(provider_id)
                // Bounded here as well as at the provider client, because *every* `ListProviders`
                // returns this column to every client.
                .bind(truncate_provider_detail(message))
                .execute(&self.pool)
                .await?;
        if updated.rows_affected() == 0 {
            return Err(ModelRegistryError::NotFound(format!(
                "no provider {provider_id} on this daemon"
            )));
        }
        Ok(())
    }

    // --- Models ------------------------------------------------------------

    /// Record a successful enumeration: the provider's catalog replaces whatever was cached, and
    /// its recorded failure is cleared — in one transaction, so the screen never shows a fresh
    /// catalog under a stale error (or the reverse).
    pub async fn record_refresh(
        &self,
        provider_id: &str,
        models: &[ModelEntry],
    ) -> Result<(), ModelRegistryError> {
        let mut tx = self.pool.begin_with(BEGIN_WRITE).await?;
        write_models(&mut tx, provider_id, models).await?;
        sqlx::query("UPDATE provider SET enumeration_error = '' WHERE provider_id = ?1")
            .bind(provider_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Replace everything cached for `provider_id` with `models` — the cache mirrors what the
    /// provider offers *now*, so a model removed on the host disappears here rather than lingering
    /// as the union of every enumeration ever run.
    pub async fn replace_models(
        &self,
        provider_id: &str,
        models: Vec<ModelEntry>,
    ) -> Result<(), ModelRegistryError> {
        let mut tx = self.pool.begin_with(BEGIN_WRITE).await?;
        write_models(&mut tx, provider_id, &models).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Every cached model on this daemon, grouped by provider in provider-creation order and, in
    /// each group, in the order that provider enumerated them.
    pub async fn list_models(&self) -> Result<Vec<ModelEntry>, ModelRegistryError> {
        let rows = sqlx::query(
            "SELECT m.provider_id, m.model_id, m.label, m.labels, m.load_state, m.size_bytes
             FROM model m
             JOIN provider p ON p.provider_id = m.provider_id
             ORDER BY p.rowid, m.ordinal",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(|row| self.row_to_model(row)).collect()
    }

    /// One cached model, or `None` when this provider's catalog does not list it.
    pub async fn model(
        &self,
        provider_id: &str,
        model_id: &str,
    ) -> Result<Option<ModelEntry>, ModelRegistryError> {
        let row = sqlx::query(
            "SELECT provider_id, model_id, label, labels, load_state, size_bytes
             FROM model WHERE provider_id = ?1 AND model_id = ?2",
        )
        .bind(provider_id)
        .bind(model_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| self.row_to_model(&row)).transpose()
    }

    /// Record the residency a provider just reported for one of its models.
    pub async fn set_load_state(
        &self,
        provider_id: &str,
        model_id: &str,
        load_state: i32,
    ) -> Result<(), ModelRegistryError> {
        sqlx::query("UPDATE model SET load_state = ?3 WHERE provider_id = ?1 AND model_id = ?2")
            .bind(provider_id)
            .bind(model_id)
            .bind(load_state)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    fn row_to_model(
        &self,
        row: &sqlx::sqlite::SqliteRow,
    ) -> Result<ModelEntry, ModelRegistryError> {
        Ok(ModelEntry {
            model_id: row.get("model_id"),
            provider_id: row.get("provider_id"),
            label: row.get("label"),
            labels: decode_list("model.labels", &row.get::<String, _>("labels"))?,
            load_state: row.get("load_state"),
            daemon_instance_id: self.daemon_instance_id.clone(),
            size_bytes: row.get::<i64, _>("size_bytes") as u64,
        })
    }

    // --- Assistants --------------------------------------------------------

    /// Define an assistant owned by `owner`. Its `name` is the `--agent` value a session is
    /// started with, so it must be unique on this daemon and must not shadow a builtin agent def.
    ///
    /// The name check, the provider check and the insert share one write transaction: separately,
    /// an assistant could be admitted against a provider that was deleted a moment earlier, and
    /// `ListAgents` would go on offering it forever.
    pub async fn create_assistant(
        &self,
        assistant: NewAssistant,
        owner: &str,
    ) -> Result<AssistantEntry, ModelRegistryError> {
        let tools = validate_tools(&assistant.tools)?;
        // The model id is what reaches the provider as `"model": …`. An empty one would produce an
        // agent def with no model at all, which fails at inference time with the provider's words
        // rather than ours. The id is *not* checked against the cached catalog: that cache is
        // empty until someone refreshes and stale whenever the host changed, so checking it would
        // refuse legitimate models — the provider row, which is checked, is the authoritative part.
        if assistant.model_id.trim().is_empty() {
            return Err(ModelRegistryError::InvalidName(
                "an assistant needs a model id".to_string(),
            ));
        }
        reject_an_oversized_system_prompt(&assistant.system_prompt)?;

        let mut tx = self.pool.begin_with(BEGIN_WRITE).await?;
        self.reject_taken_name(&mut tx, &assistant.name).await?;
        // A row pointing at a provider that does not exist could never be projected onto an agent
        // def, so it is refused at creation rather than at first use.
        let provider_exists: Option<String> =
            sqlx::query_scalar("SELECT provider_id FROM provider WHERE provider_id = ?1")
                .bind(&assistant.provider_id)
                .fetch_optional(&mut *tx)
                .await?;
        if provider_exists.is_none() {
            return Err(ModelRegistryError::NotFound(format!(
                "no provider {} on this daemon",
                assistant.provider_id
            )));
        }

        let assistant_id = format!("asst-{}", uuid::Uuid::new_v4());
        sqlx::query(
            "INSERT INTO assistant
                (assistant_id, name, label, provider_id, model_id, system_prompt, tools, owner)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(&assistant_id)
        .bind(&assistant.name)
        .bind(&assistant.label)
        .bind(&assistant.provider_id)
        .bind(&assistant.model_id)
        .bind(&assistant.system_prompt)
        .bind(encode_list(&tools))
        .bind(owner)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            already_exists_on_unique_violation(
                e,
                &format!("an assistant named '{}' already exists", assistant.name),
            )
        })?;
        tx.commit().await?;

        Ok(AssistantEntry {
            assistant_id,
            name: assistant.name,
            label: assistant.label,
            provider_id: assistant.provider_id,
            model_id: assistant.model_id,
            system_prompt: assistant.system_prompt,
            tools,
            daemon_instance_id: self.daemon_instance_id.clone(),
        })
    }

    /// Every assistant on this daemon, in creation order — every operator's, as with providers.
    pub async fn list_assistants(&self) -> Result<Vec<AssistantEntry>, ModelRegistryError> {
        let rows = sqlx::query(
            "SELECT assistant_id, name, label, provider_id, model_id, system_prompt, tools
             FROM assistant
             ORDER BY rowid",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(AssistantEntry {
                    assistant_id: row.get("assistant_id"),
                    name: row.get("name"),
                    label: row.get("label"),
                    provider_id: row.get("provider_id"),
                    model_id: row.get("model_id"),
                    system_prompt: row.get("system_prompt"),
                    tools: decode_list("assistant.tools", &row.get::<String, _>("tools"))?,
                    daemon_instance_id: self.daemon_instance_id.clone(),
                })
            })
            .collect()
    }

    /// One assistant by id.
    pub async fn assistant(
        &self,
        assistant_id: &str,
    ) -> Result<AssistantEntry, ModelRegistryError> {
        self.list_assistants()
            .await?
            .into_iter()
            .find(|a| a.assistant_id == assistant_id)
            .ok_or_else(|| {
                ModelRegistryError::NotFound(format!("no assistant {assistant_id} on this daemon"))
            })
    }

    /// Update the editable parts of an assistant `caller` owns (its name is its identity and stays
    /// put).
    pub async fn update_assistant(
        &self,
        assistant_id: &str,
        label: &str,
        system_prompt: &str,
        tools: &[String],
        caller: &str,
    ) -> Result<AssistantEntry, ModelRegistryError> {
        let tools = validate_tools(tools)?;
        // Bounded on update as well as on create: a limit only one of the two write paths enforces
        // is a limit an operator gets past by creating a small assistant and then editing it.
        reject_an_oversized_system_prompt(system_prompt)?;
        let mut tx = self.pool.begin_with(BEGIN_WRITE).await?;
        authorize(
            assistant_owner(&mut tx, assistant_id).await?,
            caller,
            &format!("assistant {assistant_id}"),
        )?;
        sqlx::query(
            "UPDATE assistant SET label = ?2, system_prompt = ?3, tools = ?4
             WHERE assistant_id = ?1",
        )
        .bind(assistant_id)
        .bind(label)
        .bind(system_prompt)
        .bind(encode_list(&tools))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.assistant(assistant_id).await
    }

    /// Remove an assistant `caller` owns, freeing its `--agent` name.
    pub async fn delete_assistant(
        &self,
        assistant_id: &str,
        caller: &str,
    ) -> Result<(), ModelRegistryError> {
        let mut tx = self.pool.begin_with(BEGIN_WRITE).await?;
        authorize(
            assistant_owner(&mut tx, assistant_id).await?,
            caller,
            &format!("assistant {assistant_id}"),
        )?;
        sqlx::query("DELETE FROM assistant WHERE assistant_id = ?1")
            .bind(assistant_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Refuse a name that is not usable as an `--agent` value, or that is already resolvable as
    /// one — a coding backend id, a configured `allowed_agents` id, a builtin def, or another
    /// assistant. Every such collision would make `--agent <name>` ambiguous.
    async fn reject_taken_name(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        name: &str,
    ) -> Result<(), ModelRegistryError> {
        // `--agent ""` is not a name at all: it reads as "no agent selected", which starts the
        // daemon's default backend rather than this assistant.
        if name.trim().is_empty() {
            return Err(ModelRegistryError::InvalidName(
                "an assistant name is required".to_string(),
            ));
        }
        if name.trim() != name {
            return Err(ModelRegistryError::InvalidName(format!(
                "'{name}' has leading or trailing whitespace; \
                 `--agent` would never match it"
            )));
        }
        // These three are reserved names, not duplicate rows: nothing in the registry is being
        // collided with, the name simply already means something else on this daemon. `InvalidName`
        // is what that is — `AlreadyExists` would tell a caller to go and delete the assistant that
        // is in the way, and there is no such assistant.
        if tddy_coder::run::BUILTIN_BACKEND_AGENT_IDS.contains(&name) {
            return Err(ModelRegistryError::InvalidName(format!(
                "'{name}' is a coding backend"
            )));
        }
        if builtin_agent_defs().iter().any(|def| def.name == name) {
            return Err(ModelRegistryError::InvalidName(format!(
                "'{name}' is a builtin agent"
            )));
        }
        if self.reserved_agent_ids.iter().any(|id| id == name) {
            return Err(ModelRegistryError::InvalidName(format!(
                "'{name}' is listed in this daemon's allowed_agents"
            )));
        }
        let existing: Option<String> =
            sqlx::query_scalar("SELECT assistant_id FROM assistant WHERE name = ?1")
                .bind(name)
                .fetch_optional(&mut **tx)
                .await?;
        match existing {
            Some(assistant_id) => Err(ModelRegistryError::AlreadyExists(format!(
                "an assistant named '{name}' already exists ({assistant_id})"
            ))),
            None => Ok(()),
        }
    }
}

/// How long an assistant's system prompt may be.
///
/// The prompt is unbounded nowhere else: it is stored on the row, returned by *every*
/// `ListAssistants`, travels with the def to every spawned session, and is re-sent to the provider
/// on every turn of every conversation. A `ListAssistants` response past ~60 KB is chunk-framed
/// over LiveKit, where one lost frame wedges the call with no error at all (see
/// [`super::error::MAX_PROVIDER_DETAIL_BYTES`]), so a handful of assistants at this ceiling still
/// fits in one frame. 8 KiB is roughly two thousand tokens of instructions — far more than any
/// system prompt this screen exists to write.
pub const MAX_SYSTEM_PROMPT_BYTES: usize = 8 * 1024;

/// Refuse a system prompt past [`MAX_SYSTEM_PROMPT_BYTES`], saying by how much.
fn reject_an_oversized_system_prompt(system_prompt: &str) -> Result<(), ModelRegistryError> {
    if system_prompt.len() > MAX_SYSTEM_PROMPT_BYTES {
        return Err(ModelRegistryError::InvalidName(format!(
            "a system prompt may be at most {MAX_SYSTEM_PROMPT_BYTES} bytes; this one is {}",
            system_prompt.len()
        )));
    }
    Ok(())
}

/// Everyone reads; only the row's owner writes, or reads its credential.
///
/// `None` is a row created before the registry recorded owners: unowned, and writable by whoever
/// gets there first. Locking those rows to nobody would strand a running daemon's own providers,
/// and assigning them to the first caller who touched one would be a guess about who set it up.
fn authorize(owner: Option<String>, caller: &str, what: &str) -> Result<(), ModelRegistryError> {
    match owner {
        None => Ok(()),
        Some(owner) if owner == caller => Ok(()),
        Some(owner) => Err(ModelRegistryError::PermissionDenied(format!(
            "{what} belongs to {owner}; every operator sees this registry, only its owner changes it"
        ))),
    }
}

/// The operator who created `assistant_id`, or [`ModelRegistryError::NotFound`] when there is no
/// such row — an update or delete that matched nothing is a failure, never a silent no-op.
async fn assistant_owner(
    tx: &mut Transaction<'_, Sqlite>,
    assistant_id: &str,
) -> Result<Option<String>, ModelRegistryError> {
    let row = sqlx::query("SELECT owner FROM assistant WHERE assistant_id = ?1")
        .bind(assistant_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| {
            ModelRegistryError::NotFound(format!("no assistant {assistant_id} on this daemon"))
        })?;
    Ok(row.get::<Option<String>, _>("owner"))
}

/// Replace `provider_id`'s cached catalog inside an open write transaction.
///
/// The provider is looked up in the same transaction as the insert, so a refresh that started
/// before a delete cannot re-create rows for a provider that is gone — and the next provider
/// minted would otherwise inherit them.
async fn write_models(
    tx: &mut Transaction<'_, Sqlite>,
    provider_id: &str,
    models: &[ModelEntry],
) -> Result<(), ModelRegistryError> {
    let provider_exists: Option<String> =
        sqlx::query_scalar("SELECT provider_id FROM provider WHERE provider_id = ?1")
            .bind(provider_id)
            .fetch_optional(&mut **tx)
            .await?;
    if provider_exists.is_none() {
        return Err(ModelRegistryError::NotFound(format!(
            "no provider {provider_id} on this daemon"
        )));
    }

    sqlx::query("DELETE FROM model WHERE provider_id = ?1")
        .bind(provider_id)
        .execute(&mut **tx)
        .await?;
    for (ordinal, model) in models.iter().enumerate() {
        sqlx::query(
            "INSERT INTO model
                (provider_id, model_id, ordinal, label, labels, load_state, size_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(provider_id)
        .bind(&model.model_id)
        .bind(ordinal as i64)
        .bind(&model.label)
        .bind(encode_list(&model.labels))
        .bind(model.load_state)
        .bind(model.size_bytes as i64)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// Mint the id for a new provider of `kind`: `prov-<kind>` while that is free, then
/// `prov-<kind>-2`, `-3`, … for further endpoints of the same kind.
///
/// Readable rather than a UUID because this id is what every model row, assistant row, log line
/// and per-row UI action is keyed by; `prov-ollama` says which endpoint went wrong where an opaque
/// id would not. An id that has ever been used stays used: a deleted provider's id is retired, not
/// recycled, so the next provider of that kind cannot inherit its history.
async fn mint_provider_id(
    tx: &mut Transaction<'_, Sqlite>,
    kind: ProviderKind,
) -> Result<String, ModelRegistryError> {
    let base = format!("prov-{}", kind_slug(kind));
    let mut taken: Vec<String> = sqlx::query_scalar("SELECT provider_id FROM provider")
        .fetch_all(&mut **tx)
        .await?;
    let retired: Vec<String> = sqlx::query_scalar("SELECT provider_id FROM retired_provider_id")
        .fetch_all(&mut **tx)
        .await?;
    taken.extend(retired);
    if !taken.contains(&base) {
        return Ok(base);
    }
    let mut suffix = 2_u32;
    loop {
        let candidate = format!("{base}-{suffix}");
        if !taken.contains(&candidate) {
            return Ok(candidate);
        }
        suffix += 1;
    }
}

/// The lower-case name of a provider kind, as it appears in a provider id.
fn kind_slug(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Unspecified => "unspecified",
        ProviderKind::Ollama => "ollama",
        ProviderKind::Openai => "openai",
        ProviderKind::Fireworks => "fireworks",
        ProviderKind::Anthropic => "anthropic",
    }
}

/// Refuse a base URL this daemon must not be pointed at.
///
/// The daemon fetches this URL with whatever credentials the row carries and echoes the response
/// back to the caller, so an unvalidated string is a request-forgery primitive: `file:///etc`,
/// a scheme reqwest cannot speak, or `https://user:pass@host` — whose userinfo would then appear
/// verbatim in the "unreachable" message the screen renders.
fn validate_base_url(base_url: &str) -> Result<(), ModelRegistryError> {
    let parsed = url::Url::parse(base_url.trim()).map_err(|e| {
        ModelRegistryError::InvalidBaseUrl(format!("'{base_url}' is not a url ({e})"))
    })?;
    if !ALLOWED_BASE_URL_SCHEMES.contains(&parsed.scheme()) {
        return Err(ModelRegistryError::InvalidBaseUrl(format!(
            "'{base_url}' uses the scheme '{}'; a provider is reached over {}",
            parsed.scheme(),
            ALLOWED_BASE_URL_SCHEMES.join(" or ")
        )));
    }
    if parsed.host_str().is_none_or(str::is_empty) {
        return Err(ModelRegistryError::InvalidBaseUrl(format!(
            "'{base_url}' names no host"
        )));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(ModelRegistryError::InvalidBaseUrl(
            "a base url must not carry credentials; store the api key in the api key field"
                .to_string(),
        ));
    }
    Ok(())
}

/// Canonicalise the tool names an assistant was given, refusing any that is not an exec-catalog
/// tool. A typo must not be quietly dropped from the set — the assistant would then run with fewer
/// tools than its author asked for, silently.
fn validate_tools(tools: &[String]) -> Result<Vec<String>, ModelRegistryError> {
    tools
        .iter()
        .map(|name| {
            SubagentTool::from_catalog_name(name)
                .map(|tool| tool.catalog_name().to_string())
                .ok_or_else(|| ModelRegistryError::UnknownTool(name.clone()))
        })
        .collect()
}

/// A uniqueness constraint losing a race is the same answer as losing the check above it, so it is
/// reported the same way rather than as an internal storage fault.
fn already_exists_on_unique_violation(e: sqlx::Error, message: &str) -> ModelRegistryError {
    match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            ModelRegistryError::AlreadyExists(message.to_string())
        }
        _ => ModelRegistryError::Storage(e),
    }
}

/// An I/O failure on the registry's own files, in the store's error vocabulary.
fn io_failure(what: &str, e: std::io::Error) -> ModelRegistryError {
    ModelRegistryError::Storage(sqlx::Error::Io(std::io::Error::new(
        e.kind(),
        format!("{what}: {e}"),
    )))
}

/// Create the database file owner-only if it does not exist yet, leaving an existing one untouched
/// (a zero-length file is a valid empty SQLite database, which is what `create_if_missing` would
/// have made anyway).
#[cfg(unix)]
fn precreate_owner_only(db_path: &Path) -> Result<(), ModelRegistryError> {
    use std::os::unix::fs::OpenOptionsExt;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(OWNER_ONLY_FILE)
        .open(db_path)
    {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(io_failure(&format!("creating {}", db_path.display()), e)),
    }
}

#[cfg(not(unix))]
fn precreate_owner_only(_db_path: &Path) -> Result<(), ModelRegistryError> {
    Ok(())
}

/// Restrict the database and the `-wal`/`-shm` files beside it to their owner.
///
/// Run after the schema exists, so files SQLite created on the way are covered, and so a database
/// an earlier daemon left world-readable is repaired on the next start. The *directory* is left
/// alone deliberately: `models.db` lives in the shared `tddy-data-dir`, which session processes
/// running as other uids read (`projects/`, the session bases), so tightening it to `0700` would
/// break them — and a `0600` file inside a `0755` directory is already unreadable by them.
#[cfg(unix)]
fn restrict_to_owner(db_path: &Path) -> Result<(), ModelRegistryError> {
    use std::os::unix::fs::PermissionsExt;
    let mut paths = vec![db_path.to_path_buf()];
    for suffix in SIBLING_SUFFIXES {
        let mut name = db_path.as_os_str().to_os_string();
        name.push(suffix);
        paths.push(std::path::PathBuf::from(name));
    }
    for path in paths {
        match std::fs::set_permissions(&path, std::fs::Permissions::from_mode(OWNER_ONLY_FILE)) {
            Ok(()) => {}
            // `-wal`/`-shm` exist only while a connection holds them; an absent one carries no
            // rows to protect.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(io_failure(
                    &format!("restricting {} to its owner", path.display()),
                    e,
                ))
            }
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn restrict_to_owner(_db_path: &Path) -> Result<(), ModelRegistryError> {
    Ok(())
}

/// Create the `provider`, `model` and `assistant` tables if they do not exist, and bring a
/// database an earlier version created up to the current columns.
async fn ensure_schema(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS provider (
            provider_id TEXT PRIMARY KEY,
            kind INTEGER NOT NULL,
            label TEXT NOT NULL,
            base_url TEXT NOT NULL UNIQUE,
            -- The api key at rest. Read only by `credential_for`; no listing query selects it.
            credential TEXT,
            -- Reserved for the env-var-reference credential mode; unused today.
            credential_ref TEXT,
            enumeration_error TEXT NOT NULL DEFAULT '',
            -- The operator who added it. NULL means a row from before the registry had owners:
            -- unowned, and writable by anyone.
            owner TEXT
        );",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS model (
            provider_id TEXT NOT NULL,
            model_id TEXT NOT NULL,
            -- Position in the provider's own enumeration, so the cache lists models the way the
            -- provider does rather than in an alphabetical order it never chose.
            ordinal INTEGER NOT NULL,
            label TEXT NOT NULL,
            labels TEXT NOT NULL DEFAULT '[]',
            load_state INTEGER NOT NULL,
            size_bytes INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (provider_id, model_id),
            FOREIGN KEY (provider_id) REFERENCES provider(provider_id)
        );",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS assistant (
            assistant_id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            label TEXT NOT NULL,
            provider_id TEXT NOT NULL,
            model_id TEXT NOT NULL,
            system_prompt TEXT NOT NULL DEFAULT '',
            tools TEXT NOT NULL DEFAULT '[]',
            owner TEXT,
            FOREIGN KEY (provider_id) REFERENCES provider(provider_id)
        );",
    )
    .execute(pool)
    .await?;
    // Every provider id ever minted on this daemon, so none is handed out twice. Kept forever:
    // one row per provider ever created costs nothing, and losing it would recycle ids again.
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS retired_provider_id (
            provider_id TEXT PRIMARY KEY
        );",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_assistant_provider ON assistant(provider_id);")
        .execute(pool)
        .await?;
    // A database created before ownership existed has the tables but not the column. The rows in
    // it stay `NULL` — unowned — rather than being attributed to whoever restarts the daemon.
    add_column_if_missing(pool, "provider", "owner", "owner TEXT").await?;
    add_column_if_missing(pool, "assistant", "owner", "owner TEXT").await?;
    Ok(())
}

/// Add `column` to `table` when an older database does not have it yet.
///
/// `table` and `column` are literals from this module — SQLite takes neither as a bound parameter.
async fn add_column_if_missing(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    declaration: &str,
) -> Result<(), sqlx::Error> {
    let columns = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await?;
    let present = columns
        .iter()
        .any(|row| row.get::<String, _>("name") == column);
    if !present {
        sqlx::query(&format!("ALTER TABLE {table} ADD COLUMN {declaration}"))
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Store a string list as a JSON array — the same shape `session_catalog` uses for its list-valued
/// columns. Serializing a `Vec<String>` cannot fail.
fn encode_list(values: &[String]) -> String {
    serde_json::to_string(values).expect("a string list always serializes to a json array")
}

/// Read back a list column. A value this store did not write is corruption, and reading it as an
/// empty list would present an assistant as having no tools (or a model as having no labels) when
/// the truth is that its row is unreadable.
fn decode_list(column: &str, encoded: &str) -> Result<Vec<String>, ModelRegistryError> {
    serde_json::from_str(encoded).map_err(|e| {
        ModelRegistryError::Storage(sqlx::Error::Decode(
            format!("column '{column}' is not a json string array ({e}): {encoded}").into(),
        ))
    })
}
