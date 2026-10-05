/**
 * The subset of nextest's filterset language the repository's scripts read: `or`, `and`, `not`,
 * parentheses and the `binary()`, `package()` and `kind(test)` predicates, each over a whole test
 * binary. That is everything `.config/rust-e2e.filterset` and the `.config/nextest.toml` overrides
 * the scripts inspect use.
 *
 * Anything else is an error, never a guess: a filter read as something narrower or wider than
 * nextest reads it puts a wrong number in a verdict, or reports a binary as parallel that nextest
 * still serialises.
 */

/** A test binary as a filterset sees it. */
export interface FilterSubject {
  /** Cargo package that owns the test target. */
  package: string;
  /** The test target's name. */
  binary: string;
}

export type PredicateName = "binary" | "package" | "kind";

export type Filter =
  | { kind: "or" | "and"; left: Filter; right: Filter }
  | { kind: "not"; inner: Filter }
  | { kind: "predicate"; name: PredicateName; arg: string };

/** A `binary()` or `package()` name a filter mentions, with the package that qualifies a binary. */
export type FilterReference =
  | { kind: "package"; package: string }
  | { kind: "binary"; binary: string; package?: string };

type Token = { kind: "open" | "close" } | { kind: "word"; word: string } | { kind: "call"; name: string; arg: string };

const OPERATORS = new Set(["or", "and", "not"]);
const SUPPORTED_PREDICATES: readonly string[] = ["binary", "package", "kind"] satisfies PredicateName[];

function isPredicateName(name: string): name is PredicateName {
  return SUPPORTED_PREDICATES.includes(name);
}

/** Parse a filterset. Throws, naming the offending text, on anything outside the subset above. */
export function parseFilterset(filterset: string): Filter {
  const tokens = tokenize(filterset);
  let at = 0;
  const describe = (token: Token | undefined) =>
    token === undefined
      ? "end of input"
      : token.kind === "open"
        ? "("
        : token.kind === "close"
          ? ")"
          : token.kind === "word"
            ? token.word
            : `${token.name}(${token.arg})`;
  const isOperator = (word: "or" | "and" | "not") => {
    const token = tokens[at];
    return token?.kind === "word" && token.word === word;
  };
  const leftAssociative = (kind: "or" | "and", next: () => Filter) => (): Filter => {
    let left = next();
    while (isOperator(kind)) {
      at++;
      left = { kind, left, right: next() };
    }
    return left;
  };
  const parseNot = (): Filter => {
    if (!isOperator("not")) return parseAtom();
    at++;
    return { kind: "not", inner: parseNot() };
  };
  const parseAtom = (): Filter => {
    const token = tokens[at++];
    if (token?.kind === "open") {
      const inner = parseOr();
      if (tokens[at++]?.kind !== "close") throw new Error("unbalanced parentheses in the filterset");
      return inner;
    }
    if (token?.kind !== "call") throw new Error(`unexpected filterset token: ${describe(token)}`);
    return predicate(token.name, token.arg);
  };
  const parseAnd = leftAssociative("and", parseNot);
  const parseOr = leftAssociative("or", parseAnd);
  const filter = parseOr();
  if (at < tokens.length) throw new Error(`unexpected filterset token: ${describe(tokens[at])}`);
  return filter;
}

/**
 * Whether `filter` selects `subject`.
 *
 * `kind(test)` is true for every subject: every caller evaluates test targets only.
 */
export function matches(filter: Filter, subject: FilterSubject): boolean {
  switch (filter.kind) {
    case "or":
      return matches(filter.left, subject) || matches(filter.right, subject);
    case "and":
      return matches(filter.left, subject) && matches(filter.right, subject);
    case "not":
      return !matches(filter.inner, subject);
    case "predicate":
      if (filter.name === "binary") return subject.binary === filter.arg;
      if (filter.name === "package") return subject.package === filter.arg;
      return true;
  }
}

/** Parse `filterset` into a predicate over test binaries. */
export function filterPredicate(filterset: string): (subject: FilterSubject) => boolean {
  const filter = parseFilterset(filterset);
  return (subject) => matches(filter, subject);
}

/**
 * Every `package()` and `binary()` name `filter` mentions, in order of appearance. A binary on one
 * side of an `and` whose other side is a `package()` predicate is qualified by that package.
 */
export function filterReferences(filter: Filter, qualifier?: string): FilterReference[] {
  switch (filter.kind) {
    case "or":
      return [...filterReferences(filter.left, qualifier), ...filterReferences(filter.right, qualifier)];
    case "not":
      return filterReferences(filter.inner, qualifier);
    case "and": {
      const owner = packageOf(filter.left) ?? packageOf(filter.right) ?? qualifier;
      return [...filterReferences(filter.left, owner), ...filterReferences(filter.right, owner)];
    }
    case "predicate":
      if (filter.name === "package") return [{ kind: "package", package: filter.arg }];
      if (filter.name === "binary") {
        return [{ kind: "binary", binary: filter.arg, ...(qualifier === undefined ? {} : { package: qualifier }) }];
      }
      return [];
  }
}

function packageOf(filter: Filter): string | undefined {
  return filter.kind === "predicate" && filter.name === "package" ? filter.arg : undefined;
}

function predicate(name: string, rawArg: string): Filter {
  if (!isPredicateName(name)) throw new Error(`unsupported filterset predicate: ${name}(${rawArg})`);
  const arg = exactName(name, rawArg.trim());
  if (name === "kind" && arg !== "test") throw new Error(`unsupported filterset predicate: ${name}(${rawArg})`);
  return { kind: "predicate", name, arg };
}

/**
 * The name a predicate argument matches exactly. nextest reads a bare `binary()` / `package()`
 * argument as a glob, so a bare name without glob syntax is exact; `=name` is exact by definition.
 * Other matchers (`~` contains, `/regex/`, `#glob`) and glob syntax are rejected.
 */
function exactName(predicateName: string, arg: string): string {
  const name = arg.startsWith("=") ? arg.slice(1) : arg;
  const unsupported = /^[~/#]/.test(name)
    ? `the ${name[0]} matcher`
    : /[*?[\]{},]/.test(name)
      ? "glob syntax"
      : name === ""
        ? "an empty argument"
        : undefined;
  if (unsupported !== undefined) {
    throw new Error(`only exact names are supported in a filterset, not ${unsupported}: ${predicateName}(${arg})`);
  }
  return name;
}

/** Split a filterset into tokens; any character that starts no token is an error naming its offset. */
function tokenize(filterset: string): Token[] {
  const tokens: Token[] = [];
  const word = /[A-Za-z_][A-Za-z0-9_-]*/y;
  const call = /([A-Za-z_][A-Za-z0-9_-]*)\(([^()]*)\)/y;
  let at = 0;
  while (at < filterset.length) {
    const c = filterset[at];
    if (/\s/.test(c)) {
      at++;
      continue;
    }
    if (c === "(" || c === ")") {
      tokens.push({ kind: c === "(" ? "open" : "close" });
      at++;
      continue;
    }
    call.lastIndex = at;
    const callMatch = call.exec(filterset);
    if (callMatch && !OPERATORS.has(callMatch[1])) {
      tokens.push({ kind: "call", name: callMatch[1], arg: callMatch[2] });
      at = call.lastIndex;
      continue;
    }
    word.lastIndex = at;
    const wordMatch = word.exec(filterset);
    if (wordMatch && OPERATORS.has(wordMatch[0])) {
      tokens.push({ kind: "word", word: wordMatch[0] });
      at = word.lastIndex;
      continue;
    }
    const found = wordMatch ? `"${wordMatch[0]}"` : `character ${JSON.stringify(c)}`;
    throw new Error(`unrecognised filterset token: ${found} at offset ${at} of ${JSON.stringify(filterset)}`);
  }
  return tokens;
}
