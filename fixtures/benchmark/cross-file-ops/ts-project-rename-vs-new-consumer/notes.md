# ts-project-rename-vs-new-consumer
A renames isLoggedIn -> isAuthenticated project-wide (includes nav.ts import
and call). B adds a new showAdminLink() in nav.ts that also calls isLoggedIn.
Text merge: B's new function references old name -> broken import chain
after A's rename.
