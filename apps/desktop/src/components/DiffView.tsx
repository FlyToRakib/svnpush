interface DiffViewProps {
  diff: string;
  label: string;
}

function lineClass(line: string): string {
  if (line.startsWith("+++") || line.startsWith("---")) {
    return "diff__line diff__line--file";
  }
  if (line.startsWith("@@")) {
    return "diff__line diff__line--hunk";
  }
  if (line.startsWith("+")) {
    return "diff__line diff__line--add";
  }
  if (line.startsWith("-")) {
    return "diff__line diff__line--del";
  }
  return "diff__line";
}

/** A unified diff, rendered as text with added and removed lines marked. */
export function DiffView({ diff, label }: DiffViewProps) {
  const lines = diff.replace(/\r?\n$/, "").split(/\r?\n/);
  return (
    <pre className="diff" aria-label={label} tabIndex={0}>
      {lines.map((line, index) => (
        // Each line is a block, so no newline character is added between them.
        <span key={index} className={lineClass(line)}>
          {line || " "}
        </span>
      ))}
    </pre>
  );
}
