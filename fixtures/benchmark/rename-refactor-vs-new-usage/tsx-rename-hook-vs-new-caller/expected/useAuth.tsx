import * as React from "react";

export function useSession() {
  return React.useContext(AuthContext);
}

export const AuthContext = React.createContext(null);

export function Header() {
  const auth = useSession();
  return <div>{auth ? "in" : "out"}</div>;
}

export function Footer() {
  const auth = useSession();
  return <footer>{auth ? "bye" : ""}</footer>;
}
