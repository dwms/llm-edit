const queue = [];
let running = false;

function enqueue(task) {
  queue.push(task);
  if (!running) {
    drain();
  }
}

function drain() {
  running = true;
  while (queue.length > 0) {
    const next = queue.shift();
    try {
      next();
    } catch (err) {
      console.error("task failed", err);
    }
  }
  running = false;
}

function size() {
  return queue.length;
}

function clear() {
  queue.length = 0;
}

function peekOrDrain() {
  if (queue.length === 0) return undefined;
  drain();
  return queue[0];
}

module.exports = { enqueue, drain, size, clear };
