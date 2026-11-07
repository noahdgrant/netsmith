// The writes of the layout file, one at a time. Each write makes the server analyse the whole project again (ADR 0061,
// "Built in stage 3": that is what a change to the file costs), so the page never has two in flight and never writes while a pointer
// moves: a drop is one write. A drop that comes while a write is on its way waits (one only, and it is made from the file as the
// first write left it, which is why a job is a change to apply and not a text); a third is refused, and the person is told.

export type Submitted = 'started' | 'queued' | 'refused';

export class OneAtATime<Job> {
  private running = false;
  private waiting: Job | null = null;

  /** `run` does a job and never throws (it says what went wrong itself); `changed` is told when `busy` or `queued` does. */
  constructor(
    private run: (job: Job) => Promise<void>,
    private changed: () => void = () => undefined,
  ) {}

  get busy(): boolean {
    return this.running;
  }

  /** Whether a job is waiting behind the one that runs. */
  get queued(): boolean {
    return this.waiting !== null;
  }

  submit(job: Job): Submitted {
    if (this.running) {
      if (this.waiting !== null) return 'refused';
      this.waiting = job;
      this.changed();
      return 'queued';
    }
    void this.start(job);
    return 'started';
  }

  private async start(first: Job) {
    this.running = true;
    this.changed();
    let job: Job | null = first;
    try {
      while (job !== null) {
        try {
          await this.run(job);
        } catch {
          // `run` reports its own failures; a job that throws anyway must not stop the one behind it.
        }
        job = this.waiting;
        this.waiting = null;
        this.changed();
      }
    } finally {
      this.running = false;
      this.changed();
    }
  }
}
