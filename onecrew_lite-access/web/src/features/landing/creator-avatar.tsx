export type CreatorId = 'ava' | 'sofia' | 'nora' | 'jonas' | 'mei' | 'imani' | 'ethan' | 'lina' | 'marcus' | 'claire';
export type CreatorChannel = 'instagram' | 'youtube';

export function CreatorAvatar({ person, large = false, channel }: { person: CreatorId; large?: boolean; channel?: CreatorChannel }) {
  const avatar = <span className={`avatar creator-avatar avatar-${person}${large ? ' large' : ''}`} aria-hidden="true" />;

  if (!channel) return avatar;

  return <span className={`creator-avatar-channel channel-${channel}`} aria-hidden="true">
    {avatar}
    <span className="creator-channel-badge">
      {channel === 'instagram'
        ? <svg viewBox="0 0 24 24"><rect x="5" y="5" width="14" height="14" rx="4"/><circle cx="12" cy="12" r="3.2"/><circle cx="16.8" cy="7.4" r="1" className="icon-dot"/></svg>
        : <svg viewBox="0 0 24 24"><path d="M20.2 7.1a2.7 2.7 0 0 0-1.9-1.9C16.6 4.8 12 4.8 12 4.8s-4.6 0-6.3.4a2.7 2.7 0 0 0-1.9 1.9A28 28 0 0 0 3.4 12a28 28 0 0 0 .4 4.9 2.7 2.7 0 0 0 1.9 1.9c1.7.4 6.3.4 6.3.4s4.6 0 6.3-.4a2.7 2.7 0 0 0 1.9-1.9 28 28 0 0 0 .4-4.9 28 28 0 0 0-.4-4.9Z"/><path d="m10 15.2 5-3.2-5-3.2v6.4Z" className="play-mark"/></svg>}
    </span>
  </span>;
}
