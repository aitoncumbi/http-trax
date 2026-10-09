"use client";

import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { xml } from "@codemirror/lang-xml";
import { EditorView } from "@codemirror/view";
import { githubDark, githubLight } from "@uiw/codemirror-theme-github";
import CodeMirror, { type ReactCodeMirrorRef } from "@uiw/react-codemirror";
import { forwardRef, useMemo } from "react";

import { useStore } from "@/lib/store";

export type CodeLanguage = "json" | "javascript" | "xml" | "text";

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
}

export const CodeEditor = forwardRef<ReactCodeMirrorRef, Props>(function CodeEditor(
  { value, onChange, language = "text", readOnly, placeholder, lineNumbers = true, wrap = false, className, minHeight },
  ref,
) {
  const theme = useStore((s) => s.theme);
  const extensions = useMemo(() => {
    const ext = [];
    if (language === "json") ext.push(json());
    if (language === "javascript") ext.push(javascript());
    if (language === "xml") ext.push(xml());
    if (wrap) ext.push(EditorView.lineWrapping);
    return ext;
  }, [language, wrap]);

  return (
    <CodeMirror
      ref={ref}
      value={value}
      onChange={onChange}
      readOnly={readOnly}
      editable={!readOnly}
      placeholder={placeholder}
      theme={theme === "dark" ? githubDark : githubLight}
      extensions={extensions}
      height="100%"
      minHeight={minHeight}
      className={className}
      basicSetup={{
        lineNumbers,
        foldGutter: lineNumbers,
        highlightActiveLine: !readOnly,
        highlightActiveLineGutter: !readOnly,
        autocompletion: !readOnly,
        bracketMatching: true,
        closeBrackets: !readOnly,
        indentOnInput: true,
        tabSize: 2,
      }}
    />
  );
});
