import { clamp, Range, fetchData } from './utils';

export class NumberService {
  private range: Range;

  constructor(range: Range) {
    this.range = range;
  }

  normalize(value: number): number {
    return clamp(value, this.range.min, this.range.max);
  }

  async load(url: string): Promise<number[]> {
    const data = await fetchData<number[]>(url);
    return data.map(v => this.normalize(v));
  }
}
