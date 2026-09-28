import { repo } from "../repositories/repo";

export function getProducts(id: string) {
  return repo.find("products", id);
}

export function listProducts() {
  return repo.all("products");
}
