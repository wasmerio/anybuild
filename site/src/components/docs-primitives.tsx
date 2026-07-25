import type { ReactNode } from "react";

export function CodeBlock({ children, label = "Terminal" }: { children: string; label?: string }) {
  return (
    <div className="my-5 overflow-hidden rounded-[14px] border border-[#2A3550] bg-[#050912]">
      <div className="border-b border-[#2A3550] bg-[#0A1020]/80 px-4 py-2 font-mono text-[11px] uppercase tracking-[0.12em] text-[#70798D]">
        {label}
      </div>
      <pre className="overflow-x-auto px-5 py-4 font-mono text-[13px] leading-6 text-[#DCE2EE]">
        <code>{children}</code>
      </pre>
    </div>
  );
}

export function InlineCode({ children }: { children: ReactNode }) {
  return (
    <code className="rounded bg-[#151E36] px-1.5 py-0.5 font-mono text-[0.88em] text-[#C9C1FF]">
      {children}
    </code>
  );
}

export function Paragraph({ children }: { children: ReactNode }) {
  return <p className="mt-4 text-[15px] leading-7 text-[#AEB7C8]">{children}</p>;
}

export function BulletList({ children }: { children: ReactNode }) {
  return (
    <ul className="mt-4 list-disc space-y-2 pl-5 text-[15px] leading-7 text-[#AEB7C8] marker:text-[#7758FF]">
      {children}
    </ul>
  );
}

export function OrderedList({ children }: { children: ReactNode }) {
  return (
    <ol className="mt-4 list-decimal space-y-2 pl-5 text-[15px] leading-7 text-[#AEB7C8] marker:font-mono marker:text-[#7758FF]">
      {children}
    </ol>
  );
}

export function Callout({
  title,
  children,
  tone = "note",
}: {
  title: string;
  children: ReactNode;
  tone?: "note" | "warning";
}) {
  const accent = tone === "warning" ? "#F59E0B" : "#7758FF";
  return (
    <aside
      className="my-5 rounded-[12px] border bg-[#0A1020]/75 px-4 py-3"
      style={{ borderColor: `${accent}66` }}
    >
      <strong className="text-[13px] font-semibold text-white">{title}</strong>
      <div className="mt-1 text-[14px] leading-6 text-[#AEB7C8]">{children}</div>
    </aside>
  );
}

export function DocsTable({ headers, rows }: { headers: string[]; rows: ReactNode[][] }) {
  return (
    <div className="my-5 overflow-x-auto rounded-[12px] border border-[#2A3550]">
      <table className="w-full min-w-[620px] border-collapse text-left text-[14px]">
        <thead className="bg-[#0A1020] text-[#DCE2EE]">
          <tr>
            {headers.map((header) => (
              <th key={header} className="border-b border-[#2A3550] px-4 py-3 font-semibold">
                {header}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, rowIndex) => (
            <tr key={rowIndex} className="border-b border-[#22304D]/70 last:border-0">
              {row.map((cell, cellIndex) => (
                <td key={cellIndex} className="px-4 py-3 align-top leading-6 text-[#AEB7C8]">
                  {cell}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function DocLink({ href, children }: { href: string; children: ReactNode }) {
  return (
    <a
      href={href}
      className="font-medium text-[#9C8BFF] underline decoration-[#7758FF]/35 underline-offset-4 transition-colors hover:text-[#C9C1FF]"
    >
      {children}
    </a>
  );
}
