# js-project-rename-3-files-html-and-tsx

A renames `showToast → flashMessage` project-scope across lib.js,
page.html, Notice.tsx (3 files of three different extensions). B adds
a new `Banner` component in Banner.tsx that calls the old name.
diff3 sees no conflict in Banner.tsx (A didn't touch it) and ships the
new component pointing at the dead name. Structured merge propagates
the project rename into B's insert_after content.

diff3 failure mode: **silent-wrong**.
