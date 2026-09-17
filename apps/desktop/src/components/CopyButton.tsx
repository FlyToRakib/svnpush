import { useState } from "react";
import { S } from "../strings";

interface CopyButtonProps {
  text: string;
  label: string;
}

/** Copies text to the clipboard and says so. */
export function CopyButton({ text, label }: CopyButtonProps) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    await navigator.clipboard.writeText(text);
    setCopied(true);
  };
  return (
    <button
      type="button"
      className="btn btn--sm"
      onClick={() => {
        void copy();
      }}
      onBlur={() => {
        setCopied(false);
      }}
    >
      {copied ? S.common.copied : label}
    </button>
  );
}
