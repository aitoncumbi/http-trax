// Safe, zero-eval JMESPath evaluator for JSON response filtering.
//
// Supported forms:
//   - Identifiers and nested sub-expressions: `foo`, `foo.bar.baz`, `"quoted.key"`
//   - Array indexing & negative indices: `items[0]`, `items[-1]`
//   - Array slicing: `items[0:3]`, `items[::2]`, `items[::-1]`
//   - List wildcard projections: `items[*].name`, `[*].id`
//   - Object wildcard projections: `people.*.name`, `*`
//   - Flatten projections: `groups[].members[].name`, `[]`
//   - Filter projections: `items[?price < \`10\`]`, `items[?active == \`true\` && stock > \`0\`]`
//   - Pipe expressions: `items[*].name | [0]`, `people | sort_by(@, &age) | [-1]`
//   - Multi-select list: `items[*].[id, name]`
//   - Multi-select hash: `items[*].{id: id, label: name}`
//   - Literals & raw strings: `` `{"a": 1}` ``, `` `10` ``, `'raw string'`
//   - Current node & expression refs: `@`, `&expr`
//   - Built-in functions: `length`, `keys`, `values`, `sort`, `sort_by`, `reverse`,
//     `contains`, `starts_with`, `ends_with`, `join`, `max`, `min`, `max_by`, `min_by`,
//     `sum`, `avg`, `not_null`, `to_string`, `to_number`, `type`, `abs`, `ceil`, `floor`,
//     `merge`, `map`

export class JmesPathError extends Error {}

type TokenType =
  | "ident"
  | "quoted"
  | "raw_string"
  | "literal"
  | "number"
  | "dot"
  | "star"
  | "lbracket"
  | "rbracket"
  | "lbrace"
  | "rbrace"
  | "lparen"
  | "rparen"
  | "flatten"
  | "filter"
  | "comma"
  | "colon"
  | "pipe"
  | "or"
  | "and"
  | "not"
  | "at"
  | "expref"
  | "comp"
  | "eof";

interface Token {
  type: TokenType;
  value: unknown;
  pos: number;
}

type AstNode =
  | { kind: "identity" }
  | { kind: "field"; name: string }
  | { kind: "index"; index: number }
  | { kind: "slice"; start: number | null; stop: number | null; step: number | null }
  | { kind: "subexpr"; left: AstNode; right: AstNode }
  | { kind: "project"; left: AstNode; right: AstNode }
  | { kind: "value_project"; left: AstNode; right: AstNode }
  | { kind: "flatten"; node: AstNode }
  | { kind: "filter"; left: AstNode; condition: AstNode; right: AstNode }
  | { kind: "pipe"; left: AstNode; right: AstNode }
  | { kind: "multi_list"; elements: AstNode[] }
  | { kind: "multi_hash"; entries: { key: string; value: AstNode }[] }
  | { kind: "literal"; value: unknown }
  | { kind: "comp"; op: string; left: AstNode; right: AstNode }
  | { kind: "and"; left: AstNode; right: AstNode }
  | { kind: "or"; left: AstNode; right: AstNode }
  | { kind: "not"; node: AstNode }
  | { kind: "expref"; node: AstNode }
  | { kind: "call"; name: string; args: AstNode[] };

export function jmesPath(data: unknown, expression: string): unknown {
  const src = expression.trim();
  if (!src) return data;
  const tokens = tokenize(src);
  const parser = new Parser(tokens);
  const ast = parser.parseExpression(0);
  parser.expect("eof");
  return evaluate(ast, data);
}

function tokenize(src: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;

  while (i < src.length) {
    const ch = src[i];
    if (ch === " " || ch === "\t" || ch === "\n" || ch === "\r") {
      i++;
      continue;
    }
    if (ch === ".") {
      tokens.push({ type: "dot", value: ".", pos: i++ });
      continue;
    }
    if (ch === "*") {
      tokens.push({ type: "star", value: "*", pos: i++ });
      continue;
    }
    if (ch === "@") {
      tokens.push({ type: "at", value: "@", pos: i++ });
      continue;
    }
    if (ch === "{") {
      tokens.push({ type: "lbrace", value: "{", pos: i++ });
      continue;
    }
    if (ch === "}") {
      tokens.push({ type: "rbrace", value: "}", pos: i++ });
      continue;
    }
    if (ch === "(") {
      tokens.push({ type: "lparen", value: "(", pos: i++ });
      continue;
    }
    if (ch === ")") {
      tokens.push({ type: "rparen", value: ")", pos: i++ });
      continue;
    }
    if (ch === ",") {
      tokens.push({ type: "comma", value: ",", pos: i++ });
      continue;
    }
    if (ch === ":") {
      tokens.push({ type: "colon", value: ":", pos: i++ });
      continue;
    }
    if (ch === "]") {
      tokens.push({ type: "rbracket", value: "]", pos: i++ });
      continue;
    }
    if (ch === "[") {
      if (src[i + 1] === "]") {
        tokens.push({ type: "flatten", value: "[]", pos: i });
        i += 2;
      } else if (src[i + 1] === "?") {
        tokens.push({ type: "filter", value: "[?", pos: i });
        i += 2;
      } else {
        tokens.push({ type: "lbracket", value: "[", pos: i++ });
      }
      continue;
    }
    if (ch === "|") {
      if (src[i + 1] === "|") {
        tokens.push({ type: "or", value: "||", pos: i });
        i += 2;
      } else {
        tokens.push({ type: "pipe", value: "|", pos: i++ });
      }
      continue;
    }
    if (ch === "&") {
      if (src[i + 1] === "&") {
        tokens.push({ type: "and", value: "&&", pos: i });
        i += 2;
      } else {
        tokens.push({ type: "expref", value: "&", pos: i++ });
      }
      continue;
    }
    if (ch === "!") {
      if (src[i + 1] === "=") {
        tokens.push({ type: "comp", value: "!=", pos: i });
        i += 2;
      } else {
        tokens.push({ type: "not", value: "!", pos: i++ });
      }
      continue;
    }
    if (ch === "=" && src[i + 1] === "=") {
      tokens.push({ type: "comp", value: "==", pos: i });
      i += 2;
      continue;
    }
    if (ch === "<" || ch === ">") {
      if (src[i + 1] === "=") {
        tokens.push({ type: "comp", value: ch + "=", pos: i });
        i += 2;
      } else {
        tokens.push({ type: "comp", value: ch, pos: i++ });
      }
      continue;
    }
    if (ch === "`") {
      const start = i++;
      let raw = "";
      while (i < src.length && src[i] !== "`") {
        if (src[i] === "\\" && src[i + 1] === "`") {
          raw += "`";
          i += 2;
        } else {
          raw += src[i++];
        }
      }
      if (i >= src.length) throw new JmesPathError(`Unclosed backtick literal at ${start}`);
      i++; // skip closing `
      try {
        tokens.push({ type: "literal", value: JSON.parse(raw), pos: start });
      } catch {
        tokens.push({ type: "literal", value: raw, pos: start });
      }
      continue;
    }
    if (ch === "'") {
      const start = i++;
      let raw = "";
      while (i < src.length && src[i] !== "'") {
        if (src[i] === "\\" && (src[i + 1] === "'" || src[i + 1] === "\\")) {
          raw += src[i + 1];
          i += 2;
        } else {
          raw += src[i++];
        }
      }
      if (i >= src.length) throw new JmesPathError(`Unclosed raw string at ${start}`);
      i++;
      tokens.push({ type: "raw_string", value: raw, pos: start });
      continue;
    }
    if (ch === '"') {
      const start = i++;
      let raw = '"';
      while (i < src.length && src[i] !== '"') {
        if (src[i] === "\\") {
          raw += src[i] + (src[i + 1] ?? "");
          i += 2;
        } else {
          raw += src[i++];
        }
      }
      if (i >= src.length) throw new JmesPathError(`Unclosed quoted identifier at ${start}`);
      raw += '"';
      i++;
      try {
        tokens.push({ type: "quoted", value: JSON.parse(raw), pos: start });
      } catch {
        throw new JmesPathError(`Invalid quoted identifier at ${start}`);
      }
      continue;
    }
    if (ch === "-" || (ch >= "0" && ch <= "9")) {
      const start = i;
      if (ch === "-") i++;
      if (i >= src.length || src[i] < "0" || src[i] > "9") {
        throw new JmesPathError(`Expected digit after '-' at ${start}`);
      }
      while (i < src.length && src[i] >= "0" && src[i] <= "9") i++;
      const num = Number.parseInt(src.slice(start, i), 10);
      tokens.push({ type: "number", value: num, pos: start });
      continue;
    }
    if (/[A-Za-z_]/.test(ch)) {
      const start = i++;
      while (i < src.length && /[A-Za-z0-9_-]/.test(src[i])) i++;
      tokens.push({ type: "ident", value: src.slice(start, i), pos: start });
      continue;
    }
    throw new JmesPathError(`Unexpected character '${ch}' at position ${i}`);
  }

  tokens.push({ type: "eof", value: "", pos: src.length });
  return tokens;
}

const BINDING_POWER: Partial<Record<TokenType, number>> = {
  pipe: 1,
  or: 2,
  and: 3,
  comp: 5,
  flatten: 9,
  filter: 21,
  star: 20,
  dot: 40,
  not: 45,
  lbracket: 55,
};

class Parser {
  private pos = 0;

  constructor(private tokens: Token[]) {}

  peek(offset = 0): Token {
    return this.tokens[this.pos + offset] ?? this.tokens[this.tokens.length - 1];
  }

  advance(): Token {
    const tok = this.peek();
    if (tok.type !== "eof") this.pos++;
    return tok;
  }

  expect(type: TokenType): Token {
    const tok = this.advance();
    if (tok.type !== type) {
      throw new JmesPathError(`Expected ${type} at position ${tok.pos}, got ${tok.type}`);
    }
    return tok;
  }

  parseExpression(rbp: number): AstNode {
    let left = this.nud();
    while (rbp < (BINDING_POWER[this.peek().type] ?? 0)) {
      left = this.led(left);
    }
    return left;
  }

  private nud(): AstNode {
    const tok = this.advance();
    switch (tok.type) {
      case "at":
        return { kind: "identity" };
      case "ident": {
        const name = String(tok.value);
        if (this.peek().type === "lparen") {
          this.advance(); // consume (
          const args: AstNode[] = [];
          if (this.peek().type !== "rparen") {
            args.push(this.parseExpression(0));
            while (this.peek().type === "comma") {
              this.advance();
              args.push(this.parseExpression(0));
            }
          }
          this.expect("rparen");
          return { kind: "call", name, args };
        }
        return { kind: "field", name };
      }
      case "quoted":
        return { kind: "field", name: String(tok.value) };
      case "raw_string":
      case "literal":
        return { kind: "literal", value: tok.value };
      case "star":
        return {
          kind: "value_project",
          left: { kind: "identity" },
          right: this.parseProjectionRhs(BINDING_POWER.star!),
        };
      case "flatten": {
        const node: AstNode = { kind: "flatten", node: { kind: "identity" } };
        const right = this.parseProjectionRhs(BINDING_POWER.flatten!);
        return { kind: "project", left: node, right };
      }
      case "filter":
        return this.parseFilter({ kind: "identity" });
      case "lbracket": {
        if (this.peek().type === "number" || this.peek().type === "colon") {
          const indexOrSlice = this.parseBracketSpec();
          if (indexOrSlice.kind === "slice") {
            return {
              kind: "project",
              left: indexOrSlice,
              right: this.parseProjectionRhs(BINDING_POWER.star!),
            };
          }
          return indexOrSlice;
        }
        if (this.peek().type === "star" && this.peek(1).type === "rbracket") {
          this.advance();
          this.advance();
          return {
            kind: "project",
            left: { kind: "identity" },
            right: this.parseProjectionRhs(BINDING_POWER.star!),
          };
        }
        return this.parseMultiSelectList();
      }
      case "lbrace":
        return this.parseMultiSelectHash();
      case "not":
        return { kind: "not", node: this.parseExpression(BINDING_POWER.not!) };
      case "expref":
        return { kind: "expref", node: this.parseExpression(BINDING_POWER.not!) };
      case "lparen": {
        const inner = this.parseExpression(0);
        this.expect("rparen");
        return inner;
      }
      default:
        throw new JmesPathError(`Unexpected token ${tok.type} at position ${tok.pos}`);
    }
  }

  private led(left: AstNode): AstNode {
    const tok = this.advance();
    switch (tok.type) {
      case "dot": {
        const next = this.peek();
        if (next.type === "star") {
          this.advance();
          return {
            kind: "value_project",
            left,
            right: this.parseProjectionRhs(BINDING_POWER.star!),
          };
        }
        const right = this.parseDotRhs(BINDING_POWER.dot!);
        return { kind: "subexpr", left, right };
      }
      case "lbracket": {
        if (this.peek().type === "star") {
          this.advance();
          this.expect("rbracket");
          return {
            kind: "project",
            left,
            right: this.parseProjectionRhs(BINDING_POWER.star!),
          };
        }
        const spec = this.parseBracketSpec();
        if (spec.kind === "slice") {
          return {
            kind: "project",
            left: { kind: "subexpr", left, right: spec },
            right: this.parseProjectionRhs(BINDING_POWER.star!),
          };
        }
        return { kind: "subexpr", left, right: spec };
      }
      case "flatten": {
        const flat: AstNode = { kind: "flatten", node: left };
        const right = this.parseProjectionRhs(BINDING_POWER.flatten!);
        return { kind: "project", left: flat, right };
      }
      case "filter":
        return this.parseFilter(left);
      case "pipe":
        return { kind: "pipe", left, right: this.parseExpression(BINDING_POWER.pipe!) };
      case "or":
        return { kind: "or", left, right: this.parseExpression(BINDING_POWER.or!) };
      case "and":
        return { kind: "and", left, right: this.parseExpression(BINDING_POWER.and!) };
      case "comp":
        return {
          kind: "comp",
          op: String(tok.value),
          left,
          right: this.parseExpression(BINDING_POWER.comp!),
        };
      default:
        throw new JmesPathError(`Unexpected infix token ${tok.type} at position ${tok.pos}`);
    }
  }

  private parseDotRhs(rbp: number): AstNode {
    const tok = this.peek();
    if (tok.type === "ident" || tok.type === "quoted") {
      return this.parseExpression(rbp);
    }
    if (tok.type === "lbracket") {
      this.advance();
      return this.parseMultiSelectList();
    }
    if (tok.type === "lbrace") {
      this.advance();
      return this.parseMultiSelectHash();
    }
    throw new JmesPathError(`Expected identifier or multi-select after '.' at position ${tok.pos}`);
  }

  private parseProjectionRhs(rbp: number): AstNode {
    const next = this.peek();
    if (next.type === "dot") {
      this.advance();
      return this.parseDotRhs(rbp);
    }
    if (next.type === "lbracket" || next.type === "flatten" || next.type === "filter") {
      return this.parseExpression(rbp);
    }
    return { kind: "identity" };
  }

  private parseFilter(left: AstNode): AstNode {
    const condition = this.parseExpression(0);
    this.expect("rbracket");
    const right = this.parseProjectionRhs(BINDING_POWER.filter!);
    return { kind: "filter", left, condition, right };
  }

  private parseBracketSpec(): { kind: "index"; index: number } | { kind: "slice"; start: number | null; stop: number | null; step: number | null } {
    let start: number | null = null;
    if (this.peek().type === "number") {
      start = Number(this.advance().value);
    }
    if (this.peek().type !== "colon") {
      if (start === null) {
        throw new JmesPathError(`Expected index or slice at position ${this.peek().pos}`);
      }
      this.expect("rbracket");
      return { kind: "index", index: start };
    }
    this.advance(); // first :
    let stop: number | null = null;
    if (this.peek().type === "number") {
      stop = Number(this.advance().value);
    }
    let step: number | null = null;
    if (this.peek().type === "colon") {
      this.advance();
      if (this.peek().type === "number") {
        step = Number(this.advance().value);
      }
    }
    this.expect("rbracket");
    return { kind: "slice", start, stop, step };
  }

  private parseMultiSelectList(): AstNode {
    const elements: AstNode[] = [];
    elements.push(this.parseExpression(0));
    while (this.peek().type === "comma") {
      this.advance();
      elements.push(this.parseExpression(0));
    }
    this.expect("rbracket");
    return { kind: "multi_list", elements };
  }

  private parseMultiSelectHash(): AstNode {
    const entries: { key: string; value: AstNode }[] = [];
    const readEntry = () => {
      const keyTok = this.advance();
      if (keyTok.type !== "ident" && keyTok.type !== "quoted") {
        throw new JmesPathError(`Expected key in multi-select hash at position ${keyTok.pos}`);
      }
      this.expect("colon");
      const value = this.parseExpression(0);
      entries.push({ key: String(keyTok.value), value });
    };
    readEntry();
    while (this.peek().type === "comma") {
      this.advance();
      readEntry();
    }
    this.expect("rbrace");
    return { kind: "multi_hash", entries };
  }
}

function isFalsey(val: unknown): boolean {
  if (val === null || val === undefined || val === false || val === "") return true;
  if (Array.isArray(val)) return val.length === 0;
  if (typeof val === "object") return Object.keys(val as Record<string, unknown>).length === 0;
  return false;
}

function deepEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (a === null || b === null || typeof a !== typeof b) return false;
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return false;
    return a.every((v, i) => deepEqual(v, b[i]));
  }
  if (typeof a === "object" && typeof b === "object") {
    const ka = Object.keys(a as Record<string, unknown>);
    const kb = Object.keys(b as Record<string, unknown>);
    if (ka.length !== kb.length) return false;
    return ka.every((k) =>
      deepEqual((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k]),
    );
  }
  return false;
}

function sliceArray(
  arr: unknown[],
  start: number | null,
  stop: number | null,
  step: number | null,
): unknown[] {
  const s = step ?? 1;
  if (s === 0) throw new JmesPathError("Slice step cannot be 0");
  const len = arr.length;
  const clamp = (n: number, lower: number, upper: number) => Math.max(lower, Math.min(upper, n));

  const from =
    start === null
      ? s > 0
        ? 0
        : len - 1
      : clamp(start < 0 ? start + len : start, s > 0 ? 0 : -1, s > 0 ? len : len - 1);
  const to =
    stop === null
      ? s > 0
        ? len
        : -1
      : clamp(stop < 0 ? stop + len : stop, s > 0 ? 0 : -1, s > 0 ? len : len - 1);

  const out: unknown[] = [];
  if (s > 0) {
    for (let i = from; i < to; i += s) out.push(arr[i]);
  } else {
    for (let i = from; i > to; i += s) out.push(arr[i]);
  }
  return out;
}

function evaluate(node: AstNode, current: unknown): unknown {
  switch (node.kind) {
    case "identity":
      return current;
    case "literal":
      return node.value;
    case "expref":
      return node;
    case "field":
      if (current && typeof current === "object" && !Array.isArray(current)) {
        const val = (current as Record<string, unknown>)[node.name];
        return val === undefined ? null : val;
      }
      return null;
    case "index": {
      if (!Array.isArray(current)) return null;
      const idx = node.index < 0 ? current.length + node.index : node.index;
      return idx >= 0 && idx < current.length ? current[idx] : null;
    }
    case "slice": {
      if (!Array.isArray(current)) return null;
      return sliceArray(current, node.start, node.stop, node.step);
    }
    case "subexpr": {
      const base = evaluate(node.left, current);
      if (base === null || base === undefined) return null;
      return evaluate(node.right, base);
    }
    case "project": {
      const base = evaluate(node.left, current);
      if (!Array.isArray(base)) return null;
      const out: unknown[] = [];
      for (const item of base) {
        const val = evaluate(node.right, item);
        if (val !== null && val !== undefined) out.push(val);
      }
      return out;
    }
    case "value_project": {
      const base = evaluate(node.left, current);
      if (!base || typeof base !== "object" || Array.isArray(base)) return null;
      const out: unknown[] = [];
      for (const val of Object.values(base as Record<string, unknown>)) {
        const projected = evaluate(node.right, val);
        if (projected !== null && projected !== undefined) out.push(projected);
      }
      return out;
    }
    case "flatten": {
      const base = evaluate(node.node, current);
      if (!Array.isArray(base)) return null;
      const out: unknown[] = [];
      for (const item of base) {
        if (Array.isArray(item)) out.push(...item);
        else if (item !== null && item !== undefined) out.push(item);
      }
      return out;
    }
    case "filter": {
      const base = evaluate(node.left, current);
      if (!Array.isArray(base)) return null;
      const out: unknown[] = [];
      for (const item of base) {
        const cond = evaluate(node.condition, item);
        if (!isFalsey(cond)) {
          const val = evaluate(node.right, item);
          if (val !== null && val !== undefined) out.push(val);
        }
      }
      return out;
    }
    case "pipe": {
      const leftVal = evaluate(node.left, current);
      return evaluate(node.right, leftVal);
    }
    case "multi_list": {
      if (current === null || current === undefined) return null;
      return node.elements.map((el) => evaluate(el, current));
    }
    case "multi_hash": {
      if (current === null || current === undefined) return null;
      const obj: Record<string, unknown> = {};
      for (const entry of node.entries) {
        obj[entry.key] = evaluate(entry.value, current);
      }
      return obj;
    }
    case "not":
      return isFalsey(evaluate(node.node, current));
    case "and": {
      const l = evaluate(node.left, current);
      return isFalsey(l) ? l : evaluate(node.right, current);
    }
    case "or": {
      const l = evaluate(node.left, current);
      return isFalsey(l) ? evaluate(node.right, current) : l;
    }
    case "comp": {
      const l = evaluate(node.left, current);
      const r = evaluate(node.right, current);
      switch (node.op) {
        case "==":
          return deepEqual(l, r);
        case "!=":
          return !deepEqual(l, r);
        case "<":
          return typeof l === "number" && typeof r === "number" ? l < r : null;
        case "<=":
          return typeof l === "number" && typeof r === "number" ? l <= r : null;
        case ">":
          return typeof l === "number" && typeof r === "number" ? l > r : null;
        case ">=":
          return typeof l === "number" && typeof r === "number" ? l >= r : null;
        default:
          return null;
      }
    }
    case "call": {
      const args = node.args.map((a) => evaluate(a, current));
      return callBuiltin(node.name, args);
    }
  }
}

function evalExpref(ref: unknown, item: unknown): unknown {
  if (ref && typeof ref === "object" && (ref as AstNode).kind === "expref") {
    return evaluate((ref as { kind: "expref"; node: AstNode }).node, item);
  }
  throw new JmesPathError("Expected expression reference (&expr)");
}

function callBuiltin(name: string, args: unknown[]): unknown {
  switch (name) {
    case "length": {
      const v = args[0];
      if (typeof v === "string" || Array.isArray(v)) return v.length;
      if (v && typeof v === "object") return Object.keys(v as Record<string, unknown>).length;
      return null;
    }
    case "keys": {
      const v = args[0];
      if (v && typeof v === "object" && !Array.isArray(v)) {
        return Object.keys(v as Record<string, unknown>);
      }
      return null;
    }
    case "values": {
      const v = args[0];
      if (v && typeof v === "object" && !Array.isArray(v)) {
        return Object.values(v as Record<string, unknown>);
      }
      return null;
    }
    case "sort": {
      const v = args[0];
      if (!Array.isArray(v)) return null;
      return [...v].sort((a, b) => {
        if (typeof a === "number" && typeof b === "number") return a - b;
        return String(a).localeCompare(String(b));
      });
    }
    case "sort_by": {
      const [arr, ref] = args;
      if (!Array.isArray(arr)) return null;
      return [...arr].sort((a, b) => {
        const ka = evalExpref(ref, a);
        const kb = evalExpref(ref, b);
        if (typeof ka === "number" && typeof kb === "number") return ka - kb;
        return String(ka).localeCompare(String(kb));
      });
    }
    case "reverse": {
      const v = args[0];
      if (Array.isArray(v)) return [...v].reverse();
      if (typeof v === "string") return v.split("").reverse().join("");
      return null;
    }
    case "contains": {
      const [haystack, needle] = args;
      if (typeof haystack === "string" && typeof needle === "string") {
        return haystack.includes(needle);
      }
      if (Array.isArray(haystack)) {
        return haystack.some((item) => deepEqual(item, needle));
      }
      return null;
    }
    case "starts_with":
      return typeof args[0] === "string" && typeof args[1] === "string"
        ? args[0].startsWith(args[1])
        : null;
    case "ends_with":
      return typeof args[0] === "string" && typeof args[1] === "string"
        ? args[0].endsWith(args[1])
        : null;
    case "join": {
      const [sep, list] = args;
      if (typeof sep !== "string" || !Array.isArray(list)) return null;
      return list.map((x) => String(x)).join(sep);
    }
    case "sum": {
      const list = args[0];
      if (!Array.isArray(list)) return null;
      return list.reduce<number>((acc, n) => acc + (typeof n === "number" ? n : 0), 0);
    }
    case "avg": {
      const list = args[0];
      if (!Array.isArray(list) || list.length === 0) return null;
      const total = list.reduce<number>((acc, n) => acc + (typeof n === "number" ? n : 0), 0);
      return total / list.length;
    }
    case "max": {
      const list = args[0];
      if (!Array.isArray(list) || list.length === 0) return null;
      return list.reduce((a, b) => (a > b ? a : b));
    }
    case "min": {
      const list = args[0];
      if (!Array.isArray(list) || list.length === 0) return null;
      return list.reduce((a, b) => (a < b ? a : b));
    }
    case "max_by": {
      const [arr, ref] = args;
      if (!Array.isArray(arr) || arr.length === 0) return null;
      return arr.reduce((best, item) =>
        (evalExpref(ref, item) as number) > (evalExpref(ref, best) as number) ? item : best,
      );
    }
    case "min_by": {
      const [arr, ref] = args;
      if (!Array.isArray(arr) || arr.length === 0) return null;
      return arr.reduce((best, item) =>
        (evalExpref(ref, item) as number) < (evalExpref(ref, best) as number) ? item : best,
      );
    }
    case "not_null":
      for (const a of args) {
        if (a !== null && a !== undefined) return a;
      }
      return null;
    case "to_string":
      return typeof args[0] === "string" ? args[0] : JSON.stringify(args[0]);
    case "to_number": {
      const n = Number(args[0]);
      return Number.isNaN(n) ? null : n;
    }
    case "type": {
      const v = args[0];
      if (v === null || v === undefined) return "null";
      if (Array.isArray(v)) return "array";
      return typeof v;
    }
    case "abs":
      return typeof args[0] === "number" ? Math.abs(args[0]) : null;
    case "ceil":
      return typeof args[0] === "number" ? Math.ceil(args[0]) : null;
    case "floor":
      return typeof args[0] === "number" ? Math.floor(args[0]) : null;
    case "merge": {
      const out: Record<string, unknown> = {};
      for (const a of args) {
        if (a && typeof a === "object" && !Array.isArray(a)) {
          Object.assign(out, a);
        }
      }
      return out;
    }
    case "map": {
      const [ref, arr] = args;
      if (!Array.isArray(arr)) return null;
      return arr.map((item) => evalExpref(ref, item));
    }
    default:
      throw new JmesPathError(`Unknown JMESPath function '${name}()'`);
  }
}
