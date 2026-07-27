export function environmentProgress(lastEvent: string): number | null {
  const match = lastEvent.match(/(?:^|\s|·)(\d{1,3})%(?:$|\s|·)/);
  if (!match) return null;
  const progress = Number(match[1]);
  return progress >= 0 && progress <= 100 ? progress : null;
}
