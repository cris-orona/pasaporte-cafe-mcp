import { createHandler } from "../hosted/adapter.js";

export const config = { maxDuration: 60 };
const handler = createHandler();
export default { fetch: handler };
export const GET = handler;
export const POST = handler;
