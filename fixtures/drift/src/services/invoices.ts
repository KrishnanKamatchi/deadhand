import { repo } from "../repositories/repo";

export function getInvoices(id: string) {
  return repo.find("invoices", id);
}

export function listInvoices() {
  return repo.all("invoices");
}
