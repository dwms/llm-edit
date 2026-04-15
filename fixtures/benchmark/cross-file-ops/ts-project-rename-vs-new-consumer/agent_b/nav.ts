import { isLoggedIn } from "./auth";

export function navItems() {
  return isLoggedIn() ? ["home", "profile"] : ["home"];
}

export function showAdminLink() {
  return isLoggedIn() && globalThis.isAdmin;
}
