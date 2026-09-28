import { add } from "@app/math/add";
import { clamp } from "~lib/clamp";
import { version } from "./meta.js";
import { helper } from "./helpers";
import React from "react";

export const main = () => add(clamp(1), helper()) + version + String(React);
