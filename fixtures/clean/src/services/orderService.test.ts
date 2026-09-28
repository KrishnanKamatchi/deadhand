import { createOrder } from "./orderService";

test("creates", () => {
  expect(createOrder("u", 10).total).toBeGreaterThan(10);
});
