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

export const Markdown = memo(function Markdown({ text }: { text: string }) {
  const [popover, setPopover] = useState<MathPopover | null>(null);
  const formulaRef = useRef<Element | null>(null);
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
    }, 120);
  };

  useEffect(() => () => {
    if (hideTimer.current) clearTimeout(hideTimer.current);
  }, []);

  const showMathCopy = (event: PointerEvent<HTMLDivElement>) => {
    const formula = (event.target as Element).closest(".katex");
    if (!formula || !event.currentTarget.contains(formula)) return;
    cancelHide();
    if (formulaRef.current === formula) return;
    const latex = formula.querySelector('annotation[encoding="application/x-tex"]')?.textContent;
    if (!latex) return;
    formulaRef.current = formula;
    const bounds = formula.getBoundingClientRect();
    setPopover({
      latex,
      left: Math.max(8, Math.min(event.clientX, window.innerWidth - 76)),
      top: bounds.top >= 38 ? bounds.top - 34 : bounds.bottom + 6,
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
