const records = new Map<string, number>();
export function writeRecord(key: string, value: number) {
  records.set(key, value);
  return true;
}
export function sweep(now: number) {
  for (const [key, value] of records) {
    if (value <= now) records.delete(key);
  }
}
