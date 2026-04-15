// background worker
const QUEUE = [];
function drainQueue() {
  while (QUEUE.length) {
    publish(QUEUE.shift());
  }
}
