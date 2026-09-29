import { isValidElement, memo, useEffect, useRef, useState, type PointerEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import ReactMarkdown, { type Components } from "react-markdown";
import rehypeKatex from "rehype-katex";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import "katex/dist/katex.min.css";
import { CodeBlock } from "../ui/CodeBlock";

type CodeProps = { className?: string; children?: ReactNode };
type MathPopover = { latex: string; left: number; top: number; label: string };

/* Raw HTML in transcripts is not rendered: react-markdown escapes it by default. */
const COMPONENTS: Components = {
  pre({ children }) {
    if (!isValidElement<CodeProps>(children)) return <pre>{children}</pre>;
    const language = /language-(\S+)/.exec(children.props.className ?? "")?.[1];
    return <CodeBlock code={String(children.props.children ?? "")} label={language} />;
  },
  a({ href, children }) {
    return (
      <a href={href} target="_blank" rel="noreferrer noopener">
        {children}
      </a>
    );
  },
};

type Box = { left: number; right: number; top: number; bottom: number };

/*
 * Visual lines of a formula, top to bottom. A display formula's `.katex` box spans the full
 * line, so the glyphs are measured instead; an inline formula may wrap across lines, and the
 * popover belongs over the fragment under the pointer rather than over their union.
 */
function formulaLines(formula: Element): Box[] {
  const range = document.createRange();
  range.selectNodeContents(formula.querySelector(".katex-html") ?? formula);
  const rects = Array.from(range.getClientRects())
    .filter((r) => r.width > 0 && r.height > 0)
    .sort((a, b) => a.top - b.top);
  const lines: Box[] = [];
  for (const r of rects) {
    const line = lines.find((l) => r.top < l.bottom && r.bottom > l.top);
    if (line) {
      line.left = Math.min(line.left, r.left);
      line.right = Math.max(line.right, r.right);
      line.top = Math.min(line.top, r.top);
      line.bottom = Math.max(line.bottom, r.bottom);
    } else {
      lines.push({ left: r.left, right: r.right, top: r.top, bottom: r.bottom });
    }
  }
  return lines.length > 0 ? lines : [formula.getBoundingClientRect()];
}

function lineAt(lines: Box[], y: number): number {
  const index = lines.findIndex((l) => y >= l.top && y <= l.bottom);
  if (index >= 0) return index;
  const distance = (l: Box) => Math.min(Math.abs(y - l.top), Math.abs(y - l.bottom));
  return lines.reduce((best, l, i) => (distance(l) < distance(lines[best]) ? i : best), 0);
}

export const Markdown = memo(function Markdown({ text }: { text: string }) {
  const [popover, setPopover] = useState<MathPopover | null>(null);
  const formulaRef = useRef<Element | null>(null);
  const lineRef = useRef(0);
  const hideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const cancelHide = () => {
    if (hideTimer.current) clearTimeout(hideTimer.current);
    hideTimer.current = null;
  };

  const hideSoon = () => {
    cancelHide();
    hideTimer.current = setTimeout(() => {
      formulaRef.current = null;
      setPopover(null);
    }, 300);
  };

  useEffect(() => () => {
    if (hideTimer.current) clearTimeout(hideTimer.current);
  }, []);

  // The popover is fixed to the viewport, so any scroll would detach it from its formula.
  const visible = popover !== null;
  useEffect(() => {
    if (!visible) return;
    const hide = () => {
      cancelHide();
      formulaRef.current = null;
      setPopover(null);
    };
    window.addEventListener("scroll", hide, { capture: true, passive: true });
    return () => window.removeEventListener("scroll", hide, { capture: true });
  }, [visible]);

  const showMathCopy = (event: PointerEvent<HTMLDivElement>) => {
    const formula = (event.target as Element).closest(".katex");
    // Leaving a formula for other text in the same message never fires the container's pointerleave.
    if (!formula || !event.currentTarget.contains(formula)) {
      if (formulaRef.current) hideSoon();
      return;
    }
    cancelHide();
    const latex = formula.querySelector('annotation[encoding="application/x-tex"]')?.textContent;
    if (!latex) return;
    const lines = formulaLines(formula);
    const line = lineAt(lines, event.clientY);
    if (formulaRef.current === formula && lineRef.current === line) return;
    formulaRef.current = formula;
    lineRef.current = line;
    const bounds = lines[line];
    setPopover({
      latex,
      left: Math.max(40, Math.min((bounds.left + bounds.right) / 2, window.innerWidth - 40)),
      // The popover is 25px tall; a 2px gap keeps it reachable from the formula.
      top: bounds.top >= 31 ? bounds.top - 27 : bounds.bottom + 2,
      label: "Copy",
    });
  };

  const copyMath = async () => {
    if (!popover) return;
    try {
      await navigator.clipboard.writeText(popover.latex);
      setPopover((current) => current?.latex === popover.latex ? { ...current, label: "Copied" } : current);
    } catch {
      setPopover((current) => current?.latex === popover.latex ? { ...current, label: "Copy failed" } : current);
    }
  };

  return (
    <>
      <div className="md" onPointerOver={showMathCopy} onPointerLeave={hideSoon}>
        <ReactMarkdown remarkPlugins={[remarkGfm, remarkMath]} rehypePlugins={[[rehypeKatex, { throwOnError: false }]]} components={COMPONENTS}>
          {text}
        </ReactMarkdown>
      </div>
      {popover && createPortal(
        <button
          type="button"
          className="math-copy-popover"
          style={{ left: popover.left, top: popover.top }}
          aria-label="Copy LaTeX formula"
          onPointerEnter={cancelHide}
          onPointerLeave={hideSoon}
          onClick={copyMath}
        >
          {popover.label}
        </button>,
        document.body,
      )}
    </>
  );
});
