import { isValidElement, memo, type ReactNode } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { CodeBlock } from "../ui/CodeBlock";

type CodeProps = { className?: string; children?: ReactNode };

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
  return (
    <div className="md">
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={COMPONENTS}>
        {text}
      </ReactMarkdown>
    </div>
  );
});
