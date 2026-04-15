# py-both-fix-same-typo
Both agents fix the same typo "Helo"->"Hello". Merger must dedupe; the
second apply would otherwise fail (no longer finds "Helo").
