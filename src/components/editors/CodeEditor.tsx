"use client";

import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { xml } from "@codemirror/lang-xml";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { EditorView } from "@codemirror/view";
import { tags as t } from "@lezer/highlight";
import CodeMirror, { type ReactCodeMirrorRef } from "@uiw/react-codemirror";
import { forwardRef, useMemo } from "react";

import type { VarScope } from "@/lib/variables";

import { variableExtensions } from "./cmVariables";

export type CodeLanguage = "json" | "javascript" | "xml" | "text";

// Colours are CSS variables, so one theme serves dark and light.
const highlight = HighlightStyle.define([
  { tag: [t.propertyName, t.attributeName], color: "var(--syn-key)" },
  { tag: [t.string, t.special(t.string), t.regexp], color: "var(--syn-str)" },
  { tag: [t.number, t.integer, t.float], color: "var(--syn-num)" },
  { tag: [t.bool, t.null, t.atom], color: "var(--syn-bool)" },
  { tag: [t.keyword, t.definitionKeyword, t.modifier, t.controlKeyword, t.operatorKeyword], color: "var(--syn-kw)" },
  { tag: [t.comment, t.lineComment, t.blockComment], color: "var(--syn-comment)", fontStyle: "italic" },
  { tag: [t.punctuation, t.bracket, t.separator, t.operator, t.derefOperator], color: "var(--syn-punct)" },
  { tag: [t.variableName], color: "var(--syn-key)" },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: "var(--syn-fn)" },
  { tag: [t.tagName, t.typeName, t.className], color: "var(--syn-kw)" },
]);

const base = EditorView.theme({
  "&": { color: "var(--text)", backgroundColor: "transparent" },
  ".cm-content": { caretColor: "var(--text)", padding: "12px 0" },
  ".cm-gutters": { backgroundColor: "transparent", color: "var(--faint)" },
  ".cm-foldGutter .cm-gutterElement": { color: "var(--dim)" },
  ".cm-matchingBracket": { backgroundColor: "var(--selected)", outline: "none" },
});

const theme = [base, syntaxHighlighting(highlight)];

interface Props {
  value: string;
  onChange?: (value: string) => void;
  language?: CodeLanguage;
  readOnly?: boolean;
  placeholder?: string;
  lineNumbers?: boolean;
  wrap?: boolean;
  className?: string;
  minHeight?: string;
  /** Highlight, complete and explain `{{variables}}` from this scope. */
  variables?: VarScope | null;
}

export const CodeEditor = forwardRef<ReactCodeMirrorRef, Props>(function CodeEditor(
  { value, onChange, language = "text", readOnly, placeholder, lineNumbers = true, wrap = false, className, minHeight, variables },
  ref,
) {
  const extensions = useMemo(() => {
    const ext = [];
    if (language === "json") ext.push(json());
    if (language === "javascript") ext.push(javascript());
    if (language === "xml") ext.push(xml());
    if (wrap) ext.push(EditorView.lineWrapping);
    if (variables) ext.push(...variableExtensions(variables));
    return ext;
  }, [language, wrap, variables]);

  return (
    <CodeMirror
      ref={ref}
      value={value}
      onChange={onChange}
      readOnly={readOnly}
      editable={!readOnly}
      placeholder={placeholder}
      theme={theme}
      extensions={extensions}
      height="100%"
      minHeight={minHeight}
      className={className}
      basicSetup={{
        lineNumbers,
        foldGutter: false,
        highlightActiveLine: false,
        highlightActiveLineGutter: false,
        autocompletion: !readOnly,
        bracketMatching: true,
        closeBrackets: !readOnly,
        indentOnInput: true,
        tabSize: 2,
      }}
    />
  );
});
