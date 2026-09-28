import { chargeCard } from "../services/billing";
import { db } from "../db/client";
import { a } from "../misc/a";
import { b } from "../misc/b";
import { c } from "../misc/c";
import { d } from "../misc/d";
import { e } from "../misc/e";
import { f } from "../misc/f";

export const helpers = {
  log(x: unknown) { console.log(x); },
  all: [a, b, c, d, e, f, chargeCard, db],
};

export function process_items(tmp: any, obj: any, x: number, y: number, z: number, w: number) {
  const data = [tmp, obj, x, y, z, w];
  return data;
}
