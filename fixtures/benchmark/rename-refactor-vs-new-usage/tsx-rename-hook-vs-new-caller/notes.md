# tsx-rename-hook-vs-new-caller

A renames the React hook `useAuth → useSession` (module scope). B adds a `Footer` component that calls `useAuth()`. After merge the Footer must call `useSession()`. Exercises rename through TSX payload containing both JSX and hook call.
