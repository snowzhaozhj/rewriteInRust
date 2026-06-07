export function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

export async function fetchData<T>(url: string): Promise<T> {
  const response = await fetch(url);
  return response.json() as Promise<T>;
}

export interface Range {
  min: number;
  max: number;
}

export type Predicate<T> = (item: T) => boolean;
