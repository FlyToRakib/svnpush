import { useState } from "react";
import { S } from "../strings";

interface CopyButtonProps {
  /** The text, or a function that builds it when clicked (for long text). */
  text: string | (() => string);
  label: string;
}

/** Copies text to the clipboard and says whether it worked. */
export function CopyButton({ text, label }: CopyButtonProps) {
  const [result, setResult] = useState<"copied" | "failed" | null>(null);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(typeof text === "function" ? text() : text);
      setResult("copied");
    } catch {
      setResult("failed");
    }
  };
  return (
    <button
      type="button"
      className="btn btn--sm"
      onClick={() => {
        void copy();
      }}
      onBlur={() => {
        setResult(null);
      }}
    >
      {result === "copied" ? S.common.copied : result === "failed" ? S.common.copyFailed : label}
    </button>
  );
}
