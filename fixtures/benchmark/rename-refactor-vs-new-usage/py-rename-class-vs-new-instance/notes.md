# py-rename-class-vs-new-instance
A renames class UserAccount -> Account. B adds make_admin() that instantiates
UserAccount. Merged textually, the new code breaks at import time.
