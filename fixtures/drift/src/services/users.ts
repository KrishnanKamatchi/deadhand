import { repo } from "../repositories/repo";

export function getUsers(id: string) {
  return repo.find("users", id);
}

export function listUsers() {
  return repo.all("users");
}
