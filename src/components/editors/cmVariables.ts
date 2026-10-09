// `{{variables}}` in CodeMirror: highlighting, completion after `{{`, and a
// hover card with the resolved value.

import type { CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { EditorState, type Extension } from "@codemirror/state";
import { Decoration, EditorView, hoverTooltip, MatchDecorator, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";

import { suggest, type VarScope } from "@/lib/variables";

function card(name: string, scope: VarScope): HTMLElement {
  const info = scope.get(name);
  const el = document.createElement("div");
  el.className = "cm-var-card";
  const title = el.appendChild(document.createElement("div"));
  title.className = "cm-var-card-name";
  title.textContent = `{{${name}}}`;
  const value = el.appendChild(document.createElement("div"));
  const meta = el.appendChild(document.createElement("div"));
  meta.className = "cm-var-card-meta";
  if (info) {
    value.className = "cm-var-card-value";
    value.textContent = info.source === "dynamic" ? info.value : info.secret ? "••••••••" : info.value || "empty";
    meta.textContent = info.label + (info.secret ? " · secret" : "");
  } else {
    value.className = "cm-var-card-missing";
    value.textContent = "Not defined in the active environment, the collection or the workspace.";
  }
  return el;
}

export function variableExtensions(scope: VarScope): Extension[] {
  const decorator = new MatchDecorator({
    regexp: /\{\{([^{}]*)\}\}/g,
    decoration: (m) => Decoration.mark({ class: scope.has(m[1].trim()) ? "var-token" : "var-token-missing" }),
  });
  const highlight = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(view: EditorView) {
        this.decorations = decorator.createDeco(view);
      }
      update(u: ViewUpdate) {
        this.decorations = decorator.updateDeco(u, this.decorations);
      }
    },
    { decorations: (v) => v.decorations },
  );

  const complete = (ctx: CompletionContext): CompletionResult | null => {
    const m = ctx.matchBefore(/\{\{\s*[\w.$-]*$/);
    if (!m) return null;
    const query = m.text.replace(/^\{\{\s*/, "");
    const after = ctx.state.sliceDoc(ctx.pos, ctx.pos + 2);
    return {
      from: m.from,
      filter: false,
      options: suggest(scope, query, 30).map((v) => ({
        label: `{{${v.name}}}`,
        detail: v.source === "dynamic" ? v.value : v.secret ? "secret" : v.value,
        type: "variable",
        apply: (view, _c, from, to) => {
          const insert = `{{${v.name}}}`;
          const end = after === "}}" ? to + 2 : to;
          view.dispatch({ changes: { from, to: end, insert }, selection: { anchor: from + insert.length } });
        },
      })),
    };
  };

  const hover = hoverTooltip((view, pos) => {
    const line = view.state.doc.lineAt(pos);
    const re = /\{\{([^{}]*)\}\}/g;
    let m: RegExpExecArray | null;
    while ((m = re.exec(line.text))) {
      const from = line.from + m.index;
      const to = from + m[0].length;
      if (pos >= from && pos <= to) {
        const name = m[1].trim();
        return { pos: from, end: to, above: false, create: () => ({ dom: card(name, scope) }) };
      }
    }
    return null;
  });

  return [highlight, hover, EditorState.languageData.of(() => [{ autocomplete: complete }])];
}
