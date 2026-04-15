import * as React from "react";

export function useAuth() {
  return React.useContext(AuthContext);
}

export const AuthContext = React.createContext(null);

export function Header() {
  const auth = useAuth();
  return <div>{auth ? "in" : "out"}</div>;
}

export function Footer() {
  const auth = useAuth();
  return <footer>{auth ? "bye" : ""}</footer>;
}
