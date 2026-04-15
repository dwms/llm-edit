import { isLoggedIn } from "./auth";

export function navItems() {
  return isLoggedIn() ? ["home", "profile"] : ["home"];
}
