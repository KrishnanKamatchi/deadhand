import { saveOrder, findOrders } from "../repositories/orderRepository";
import { applyTax } from "./pricing";
import type { Order } from "../models/order";

export function createOrder(userId: string, total: number): Order {
  const order = { id: `${userId}-${Date.now()}`, userId, total: applyTax(total) };
  saveOrder(order);
  return order;
}

export function listOrders(userId: string): Order[] {
  return findOrders(userId);
}
