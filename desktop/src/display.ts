export const SESSION_TITLE_LIMIT = 32;

export function sessionTitle(title: string) {
  return (
    Array.from(title.replace(/\s+/gu, ' ').trim()).slice(0, SESSION_TITLE_LIMIT).join('') ||
    '新会话'
  );
}

export function replayName(name: string) {
  return name.replace(/\.json(?=$| · )/gi, '');
}
