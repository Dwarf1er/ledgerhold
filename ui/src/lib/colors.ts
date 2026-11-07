// Avatar colours a person can have. Keys match crates/store (PERSON_COLORS); every shade is dark
// enough for white initials, so one set works in both light and dark themes.
export const PERSON_COLORS = [
  { key: 'blue', label: 'Blue', hex: '#2563eb' },
  { key: 'green', label: 'Green', hex: '#15803d' },
  { key: 'orange', label: 'Orange', hex: '#c2410c' },
  { key: 'red', label: 'Red', hex: '#b91c1c' },
  { key: 'purple', label: 'Purple', hex: '#7e22ce' },
  { key: 'pink', label: 'Pink', hex: '#be185d' },
  { key: 'teal', label: 'Teal', hex: '#0f766e' },
  { key: 'slate', label: 'Slate', hex: '#475569' },
] as const;

export type PersonColor = (typeof PERSON_COLORS)[number]['key'];

export const colorHex = (key: string | null | undefined) =>
  PERSON_COLORS.find((c) => c.key === key)?.hex ?? PERSON_COLORS[PERSON_COLORS.length - 1].hex;

/** The first colour nobody in the household has yet (what the server picks when none is chosen). */
export function nextFreeColor(people: { color: string }[]): PersonColor {
  const used = new Set(people.map((p) => p.color));
  return (PERSON_COLORS.find((c) => !used.has(c.key)) ?? PERSON_COLORS[people.length % PERSON_COLORS.length]).key;
}
