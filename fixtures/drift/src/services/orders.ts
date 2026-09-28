import { repo } from "../repositories/repo";

export function getOrders(id: string) {
  return repo.find("orders", id);
}

export function listOrders() {
  return repo.all("orders");
}
