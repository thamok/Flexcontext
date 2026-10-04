export class Worker {
  private active = new Set<string>();
  private abandoned = new Set<string>();
  private counters = new Map<string, number>();
  pump(id: string) {
    if (this.abandoned.has(id)) return false;
    this.active.add(id);
    return true;
  }
  finish(id: string) {
    this.active.delete(id);
  }
  step(id: string) {
    this.abandoned.add(id);
    this.active.delete(id);
  }
  cancelMetrics(id: string) {
    return this.counters.get(id) ?? 0;
  }
}
