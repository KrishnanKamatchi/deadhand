import { repo } from "../repositories/repo";

export function getCarts(id: string) {
  return repo.find("carts", id);
}

export function listCarts() {
  return repo.all("carts");
}
