import { repo } from "../repositories/repo";

export function getPayments(id: string) {
  return repo.find("payments", id);
}

export function listPayments() {
  return repo.all("payments");
}
