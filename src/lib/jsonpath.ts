// A small JSONPath evaluator for filtering responses. No eval: filters are
// parsed into a tiny expression tree.
//
// Supported: $ (optional), .name, ['name'], [0], [-1], [0,2], ['a','b'],
// [start:end:step], [*], .*, ..name, ..*, and filters such as
// [?(@.price < 10)], [?(@.isbn)], [?(@.name == 'Ada' && @.age >= 18)],
// [?(@.email =~ /@example\.com$/i)].

type Value = unknown;

type Step =
  | { kind: "key"; name: string }
  | { kind: "keys"; names: string[] }
  | { kind: "index"; indexes: number[] }
  | { kind: "slice"; start?: number; end?: number; step?: number }
  | { kind: "wildcard" }
  | { kind: "descend"; then: Step }
  | { kind: "filter"; expr: Expr };

type Operand = { kind: "path"; steps: Step[] } | { kind: "literal"; value: Value } | { kind: "regex"; re: RegExp };
type Expr =
  | { kind: "exists"; operand: Operand }
  | { kind: "compare"; op: string; left: Operand; right: Operand }
  | { kind: "and" | "or"; left: Expr; right: Expr }
  | { kind: "not"; expr: Expr };

export class JsonPathError extends Error {}

class Parser {
  i = 0;
  constructor(readonly src: string) {}

  peek(n = 0) {
    return this.src[this.i + n];
  }
  eof() {
    return this.i >= this.src.length;
  }
  ws() {
    while (!this.eof() && /\s/.test(this.peek())) this.i++;
  }
  expect(ch: string) {
    this.ws();
    if (this.peek() !== ch) throw new JsonPathError(`expected "${ch}" at ${this.i + 1}`);
    this.i++;
  }
  name(): string {
    const m = /^[\w$-]+/.exec(this.src.slice(this.i));
    if (!m) throw new JsonPathError(`expected a name at ${this.i + 1}`);
    this.i += m[0].length;
    return m[0];
  }
  string(): string {
    const quote = this.peek();
    this.i++;
    let out = "";
    while (!this.eof() && this.peek() !== quote) {
      if (this.peek() === "\\") this.i++;
      out += this.peek();
      this.i++;
    }
    if (this.eof()) throw new JsonPathError("unterminated string");
    this.i++;
    return out;
  }
  number(): number {
    const m = /^-?\d+(\.\d+)?/.exec(this.src.slice(this.i));
    if (!m) throw new JsonPathError(`expected a number at ${this.i + 1}`);
    this.i += m[0].length;
    return Number(m[0]);
  }

  /** Steps until the end, or until `stop` matches (inside filters). */
  steps(stop?: RegExp): Step[] {
    const steps: Step[] = [];
    for (;;) {
      this.ws();
      if (this.eof() || (stop && stop.test(this.src.slice(this.i)))) return steps;
      const ch = this.peek();
      if (ch === "." && this.peek(1) === ".") {
        this.i += 2;
        let next: Step;
        if (this.peek() === "*") {
          this.i++;
          next = { kind: "wildcard" };
        } else if (this.peek() === "[") next = this.bracket();
        else next = { kind: "key", name: this.name() };
        steps.push({ kind: "descend", then: next });
      } else if (ch === ".") {
        this.i++;
        if (this.peek() === "*") {
          this.i++;
          steps.push({ kind: "wildcard" });
        } else steps.push({ kind: "key", name: this.name() });
      } else if (ch === "[") steps.push(this.bracket());
      else if (stop) return steps;
      else throw new JsonPathError(`unexpected "${ch}" at ${this.i + 1}`);
    }
  }

  bracket(): Step {
    this.expect("[");
    this.ws();
    const ch = this.peek();
    let step: Step;
    if (ch === "*") {
      this.i++;
      step = { kind: "wildcard" };
    } else if (ch === "?") {
      this.i++;
      this.ws();
      const paren = this.peek() === "(";
      if (paren) this.i++;
      const expr = this.or();
      if (paren) this.expect(")");
      step = { kind: "filter", expr };
    } else if (ch === "'" || ch === '"') {
      const names = [this.string()];
      this.ws();
      while (this.peek() === ",") {
        this.i++;
        this.ws();
        names.push(this.string());
        this.ws();
      }
      step = names.length === 1 ? { kind: "key", name: names[0] } : { kind: "keys", names };
    } else {
      // index, list or slice
      const read = () => {
        this.ws();
        return /[-\d]/.test(this.peek() ?? "") ? this.number() : undefined;
      };
      const first = read();
      this.ws();
      if (this.peek() === ":") {
        this.i++;
        const end = read();
        this.ws();
        let stepN: number | undefined;
        if (this.peek() === ":") {
          this.i++;
          stepN = read();
        }
        step = { kind: "slice", start: first, end, step: stepN };
      } else {
        if (first === undefined) throw new JsonPathError(`expected an index at ${this.i + 1}`);
        const indexes = [first];
        while (this.peek() === ",") {
          this.i++;
          const n = read();
          if (n === undefined) throw new JsonPathError(`expected an index at ${this.i + 1}`);
          indexes.push(n);
          this.ws();
        }
        step = { kind: "index", indexes };
      }
    }
    this.expect("]");
    return step;
  }

  or(): Expr {
    let left = this.and();
    for (;;) {
      this.ws();
      if (this.src.startsWith("||", this.i)) {
        this.i += 2;
        left = { kind: "or", left, right: this.and() };
      } else return left;
    }
  }
  and(): Expr {
    let left = this.unary();
    for (;;) {
      this.ws();
      if (this.src.startsWith("&&", this.i)) {
        this.i += 2;
        left = { kind: "and", left, right: this.unary() };
      } else return left;
    }
  }
  unary(): Expr {
    this.ws();
    if (this.peek() === "!" && this.peek(1) !== "=") {
      this.i++;
      return { kind: "not", expr: this.unary() };
    }
    if (this.peek() === "(") {
      this.i++;
      const e = this.or();
      this.expect(")");
      return e;
    }
    const left = this.operand();
    this.ws();
    const m = /^(==|!=|<=|>=|=~|<|>)/.exec(this.src.slice(this.i));
    if (!m) return { kind: "exists", operand: left };
    this.i += m[0].length;
    return { kind: "compare", op: m[0], left, right: this.operand() };
  }
  operand(): Operand {
    this.ws();
    const ch = this.peek();
    if (ch === "@" || ch === "$") {
      this.i++;
      return { kind: "path", steps: this.steps(/^\s*(==|!=|<=|>=|=~|<|>|&&|\|\||\)|\])/) };
    }
    if (ch === "'" || ch === '"') return { kind: "literal", value: this.string() };
    if (ch === "/") {
      this.i++;
      let body = "";
      while (!this.eof() && this.peek() !== "/") {
        if (this.peek() === "\\") {
          body += this.peek();
          this.i++;
        }
        body += this.peek();
        this.i++;
      }
      this.i++;
      const flags = /^[gimsuy]*/.exec(this.src.slice(this.i))?.[0] ?? "";
      this.i += flags.length;
      try {
        return { kind: "regex", re: new RegExp(body, flags) };
      } catch {
        throw new JsonPathError(`invalid regular expression /${body}/`);
      }
    }
    for (const [word, value] of [["true", true], ["false", false], ["null", null]] as const) {
      if (this.src.startsWith(word, this.i)) {
        this.i += word.length;
        return { kind: "literal", value };
      }
    }
    return { kind: "literal", value: this.number() };
  }
}

function children(v: Value): Value[] {
  if (Array.isArray(v)) return v;
  if (v && typeof v === "object") return Object.values(v);
  return [];
}

function descendants(v: Value): Value[] {
  const out: Value[] = [v];
  for (const c of children(v)) out.push(...descendants(c));
  return out;
}

function apply(step: Step, nodes: Value[], root: Value): Value[] {
  const out: Value[] = [];
  for (const node of nodes) {
    switch (step.kind) {
      case "key":
        if (node && typeof node === "object" && !Array.isArray(node) && step.name in node) out.push((node as Record<string, Value>)[step.name]);
        else if (Array.isArray(node) && step.name === "length") out.push(node.length);
        break;
      case "keys":
        for (const n of step.names) if (node && typeof node === "object" && n in node) out.push((node as Record<string, Value>)[n]);
        break;
      case "index":
        if (Array.isArray(node)) {
          for (const i of step.indexes) {
            const at = i < 0 ? node.length + i : i;
            if (at >= 0 && at < node.length) out.push(node[at]);
          }
        }
        break;
      case "slice":
        if (Array.isArray(node)) {
          const n = node.length;
          const by = step.step ?? 1;
          if (by === 0) break;
          const norm = (x: number) => (x < 0 ? Math.max(0, n + x) : Math.min(n, x));
          if (by > 0) {
            for (let i = norm(step.start ?? 0); i < norm(step.end ?? n); i += by) out.push(node[i]);
          } else {
            for (let i = step.start === undefined ? n - 1 : norm(step.start); i > (step.end === undefined ? -1 : norm(step.end)); i += by) out.push(node[i]);
          }
        }
        break;
      case "wildcard":
        out.push(...children(node));
        break;
      case "descend":
        for (const d of descendants(node)) out.push(...apply(step.then, [d], root));
        break;
      case "filter":
        for (const c of children(node)) if (test(step.expr, c, root)) out.push(c);
        break;
    }
  }
  return out;
}

function evalOperand(o: Operand, current: Value, root: Value): Value[] {
  if (o.kind === "literal") return [o.value];
  if (o.kind === "regex") return [o.re];
  return o.steps.reduce<Value[]>((nodes, s) => apply(s, nodes, root), [current]);
}

function compare(op: string, a: Value, b: Value): boolean {
  switch (op) {
    case "==":
      return a === b || (typeof a === "object" && JSON.stringify(a) === JSON.stringify(b));
    case "!=":
      return !compare("==", a, b);
    case "<":
      return typeof a === typeof b && (a as number) < (b as number);
    case "<=":
      return typeof a === typeof b && (a as number) <= (b as number);
    case ">":
      return typeof a === typeof b && (a as number) > (b as number);
    case ">=":
      return typeof a === typeof b && (a as number) >= (b as number);
    case "=~":
      return typeof a === "string" && b instanceof RegExp && b.test(a);
  }
  return false;
}

function test(e: Expr, current: Value, root: Value): boolean {
  switch (e.kind) {
    case "exists":
      return evalOperand(e.operand, current, root).length > 0;
    case "compare": {
      const left = evalOperand(e.left, current, root);
      const right = evalOperand(e.right, current, root);
      return left.some((a) => right.some((b) => compare(e.op, a, b)));
    }
    case "and":
      return test(e.left, current, root) && test(e.right, current, root);
    case "or":
      return test(e.left, current, root) || test(e.right, current, root);
    case "not":
      return !test(e.expr, current, root);
  }
}

/** All values matching `path` in `data`. Throws JsonPathError on bad syntax. */
export function jsonPath(data: Value, path: string): Value[] {
  let src = path.trim();
  if (!src) return [data];
  if (src.startsWith("$")) src = src.slice(1);
  else if (!src.startsWith(".") && !src.startsWith("[")) src = "." + src;
  const p = new Parser(src);
  const steps = p.steps();
  return steps.reduce<Value[]>((nodes, s) => apply(s, nodes, data), [data]);
}
