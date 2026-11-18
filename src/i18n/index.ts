// Translation system.
//
// `t("nav.home")` returns the message for the active language and falls back to
// English for any message a language has not translated yet. Keys are checked
// by TypeScript, so a mistyped key does not compile.
//
// To add a language: create `<code>.ts` exporting a `DeepPartial<Messages>`,
// register it in CATALOGS below, and add the code to SUPPORTED_LANGUAGES in
// src-tauri/src/settings.rs.

import { en } from "./en";

type Widen<T> = { [K in keyof T]: T[K] extends string ? string : Widen<T[K]> };
export type Messages = Widen<typeof en>;
export type DeepPartial<T> = { [K in keyof T]?: T[K] extends string ? string : DeepPartial<T[K]> };

type Plural = { one: string; other: string };
type Leaves<T, P extends string = ""> = {
  [K in keyof T & string]: T[K] extends string
    ? `${P}${K}`
    : T[K] extends Plural
      ? never
      : Leaves<T[K], `${P}${K}.`>;
}[keyof T & string];
type PluralLeaves<T, P extends string = ""> = {
  [K in keyof T & string]: T[K] extends string
    ? never
    : T[K] extends Plural
      ? `${P}${K}`
      : PluralLeaves<T[K], `${P}${K}.`>;
}[keyof T & string];

export type MessageKey = Leaves<Messages>;
export type PluralKey = PluralLeaves<Messages>;
type Params = Record<string, string | number>;

const CATALOGS: Record<string, DeepPartial<Messages>> = { en };
let active: DeepPartial<Messages> = en;
let activeCode = "en";

export function setLanguage(code: string): void {
  activeCode = code in CATALOGS ? code : "en";
  active = CATALOGS[activeCode];
}

export function currentLanguage(): string {
  return activeCode;
}

/** Registers a catalog (used by tests and by future language files). */
export function registerLanguage(code: string, messages: DeepPartial<Messages>): void {
  CATALOGS[code] = messages;
}

function lookup(catalog: unknown, key: string): unknown {
  let node = catalog;
  for (const part of key.split(".")) {
    if (node === null || typeof node !== "object") return undefined;
    node = (node as Record<string, unknown>)[part];
  }
  return node;
}

function fill(template: string, params?: Params): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (whole, name: string) =>
    name in params ? String(params[name]) : whole,
  );
}

export function t(key: MessageKey, params?: Params): string {
  const found = lookup(active, key) ?? lookup(en, key);
  return typeof found === "string" ? fill(found, params) : key;
}

/** Plural message: picks `one` or `other` by the language's own rules. */
export function tn(key: PluralKey, count: number, params?: Params): string {
  const found = (lookup(active, key) ?? lookup(en, key)) as Partial<Plural> | undefined;
  const fallback = lookup(en, key) as Plural;
  const rule = new Intl.PluralRules(activeCode).select(count);
  const template = (rule === "one" ? found?.one : found?.other) ?? found?.other ?? fallback.other;
  return fill(template, { count, ...params });
}
