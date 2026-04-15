// background worker
const QUEUE = [];
function drainQueue() {
  while (QUEUE.length) {
    dispatch(QUEUE.shift());
  }
}
