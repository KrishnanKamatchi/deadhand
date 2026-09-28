import type { Order } from "../models/order";

const orders: Order[] = [];

export function saveOrder(order: Order): void {
  orders.push(order);
}

export function findOrders(userId: string): Order[] {
  return orders.filter((order) => order.userId === userId);
}
