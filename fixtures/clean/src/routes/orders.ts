import { createOrder, listOrders } from "../services/orderService";
import type { Order } from "../models/order";

export function getOrders(userId: string): Order[] {
  return listOrders(userId);
}

export function postOrder(userId: string, total: number): Order {
  return createOrder(userId, total);
}
