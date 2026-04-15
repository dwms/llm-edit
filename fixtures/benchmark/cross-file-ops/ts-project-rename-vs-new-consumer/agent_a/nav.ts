import { isAuthenticated } from "./auth";

export function navItems() {
  return isAuthenticated() ? ["home", "profile"] : ["home"];
}
