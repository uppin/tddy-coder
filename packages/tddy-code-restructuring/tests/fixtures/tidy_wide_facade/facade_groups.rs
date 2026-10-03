use import_text::{choose_import, parent_module, without_dead_imports, occurrences_of, import_order, reached_through_qualifier, already_bound, imported_paths, use_tree, group_members, UnresolvedName};

mod lsp_edits;
use lsp_edits::{token_type_index, unresolved_in, edits_for, workspace_edits_for, read_edit, apply_lsp_edit, offset_of, position_at, lsp_position, lsp_range};

mod placeholder_checks;
use placeholder_checks::{refuse_inferred_placeholder, carries_placeholder_type, refuse_residual_placeholder, Block, declares, placeholder_sites, is_identifier_byte};

mod seam_survey;
use seam_survey::{minimal_edits, Reach, MovedItem, refuse_stranded, items_relocated_within, impls_cut_through};

mod facade;
use facade::{impl_widenings, refuse_mangled_rewrite, facade_will_bind, facade_lines, empty_facade_note};

mod module_text;
use module_text::{refuse_module_name_taken, attribute_path_names, refuse_split_attribute_paths, with_facade, ModuleBlock, module_bounds, alias_target, parent_binding, aliased_bindings, with_module_import};

mod visibility;
use visibility::{WIDENED, restore_visibility, reaches_through_module, declares_at_widened_visibility, declares_item, refuse_partial_relocation};

mod line_diff;
use line_diff::{changed_regions};
