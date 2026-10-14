import { CreatorAvatar, type CreatorId } from "../creator-avatar";
import type { Fact, ProductPreviewContent } from "./content/types";

const creators = [
  { person: "ava" as CreatorId, metaKey: "ava" as const, name: "Ava Morgan", fit: "94%" },
  { person: "jonas" as CreatorId, metaKey: "jonas" as const, name: "Jonas Kim", fit: "89%" },
];

export function ProductPreview({ copy }: { copy: ProductPreviewContent }) {
  return (
    <div className="product-preview" aria-label={copy.aria}>
      <div className="preview-bar"><strong>{copy.campaign}</strong><span>{copy.checked}</span></div>
      <div className="preview-grid">
        <section className="preview-creators">
          <header><span>{copy.candidates}</span><b>{copy.matches}</b></header>
          {creators.map((creator, index) => (
            <article className={index === 0 ? "selected" : ""} key={creator.name}>
              <CreatorAvatar person={creator.person} />
              <div><strong>{creator.name}</strong><small>{copy.creatorMeta[creator.metaKey]}</small></div>
              <b>{creator.fit}</b>
            </article>
          ))}
          <div className="reason"><strong>{copy.why}</strong><p>{copy.reason}</p></div>
        </section>
        <section className="preview-conversation">
          <header><div><strong>Ava Morgan</strong><small>{copy.channels}</small></div><span>{copy.replied}</span></header>
          <div className="message from-us">{copy.outgoing}</div>
          <div className="message from-creator">{copy.incoming}</div>
          <div className="draft">{copy.drafted} <span className="demo-action">{copy.draftAction}</span></div>
        </section>
        <aside className="preview-agent">
          <header><span className="agent-ring" aria-hidden="true" /><div><strong>{copy.next}</strong><small>{copy.approval}</small></div></header>
          <div className="intent"><small>{copy.intent}</small><strong>{copy.interested}</strong><p>{copy.intentBody}</p></div>
          <ol><li className="done">{copy.steps[0]}</li><li className="current">{copy.steps[1]}</li><li>{copy.steps[2]}</li></ol>
          <span className="approve demo-action">{copy.approve}</span>
        </aside>
      </div>
    </div>
  );
}

export function ProductFactTable({ facts }: { facts: Fact[] }) {
  return <dl className="fact-table">{facts.map(({ term, detail }) => <div key={term}><dt>{term}</dt><dd>{detail}</dd></div>)}</dl>;
}
