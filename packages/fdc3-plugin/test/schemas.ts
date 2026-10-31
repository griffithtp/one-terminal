// Validates messages against the JSON schemas published for FDC3 3.0.
import Ajv, { type ValidateFunction } from "ajv";
import addFormats from "ajv-formats";
import { readdirSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";

const require = createRequire(import.meta.url);
const schemaRoot = join(dirname(require.resolve("@finos/fdc3-schema/package.json")), "dist/schemas");

const ajv = new Ajv({ strict: false, allErrors: true, validateFormats: true });
addFormats(ajv);

// fdc3-context's exports map hides package.json, so locate it beside fdc3-schema.
const contextRoot = join(
  dirname(require.resolve("@finos/fdc3-schema/package.json")),
  "../fdc3-context/dist/schemas/context"
);
for (const file of readdirSync(contextRoot)) {
  if (file.endsWith(".schema.json")) {
    ajv.addSchema(JSON.parse(readFileSync(join(contextRoot, file), "utf8")));
  }
}
const names: string[] = [];
for (const file of readdirSync(join(schemaRoot, "api"))) {
  if (file.endsWith(".schema.json")) {
    ajv.addSchema(JSON.parse(readFileSync(join(schemaRoot, "api", file), "utf8")));
    names.push(file.replace(".schema.json", ""));
  }
}

const baseId = "https://fdc3.finos.org/schemas/next/api/";

const definitions = JSON.parse(readFileSync(join(schemaRoot, "api/api.schema.json"), "utf8")).definitions;
const specErrors = new Set<string>(
  ["ChannelError", "OpenError", "ResolveError", "ResultError", "CloseError"].flatMap(
    (name) => definitions[name].enum as string[]
  )
);
const validators = new Map<string, ValidateFunction>();

/** Returns the schema errors for `message`, or `null` if it's valid. */
export function schemaErrors(message: { type: string; payload?: { error?: unknown } }): string | null {
  if (!names.includes(message.type)) return `no schema for message type ${message.type}`;
  // agentResponse.schema.json in @finos/fdc3-schema 3.0.0 is wrong for errors: its
  // payload `oneOf` has a branch accepting any object, so a valid `{ error }` matches
  // twice and is rejected, while an unknown error string passes. Check errors against
  // the spec's own enums instead.
  const error = message.payload?.error;
  if (error !== undefined) {
    return specErrors.has(error as string) ? null : `unknown error ${String(error)}`;
  }
  let validate = validators.get(message.type);
  if (!validate) {
    validate = ajv.getSchema(`${baseId}${message.type}.schema.json`)!;
    validators.set(message.type, validate);
  }
  if (validate(message)) return null;
  return ajv.errorsText(validate.errors);
}
