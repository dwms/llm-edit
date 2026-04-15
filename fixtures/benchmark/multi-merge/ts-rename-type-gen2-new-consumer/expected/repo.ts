import { User } from "./types";

export function firstUser(all: User[]): User {
  return all[0];
}

export function lastUser(all: User[]): User {
  return all[all.length - 1];
}
