// Types for the jest-dom matchers registered in setup.ts. jest-dom's own
// `@testing-library/jest-dom/vitest` types target Vitest's older `Assertion<T>` and the
// repo-root copy of vitest; Vitest 5's extension point is `Matchers<R, T>`.
import type { TestingLibraryMatchers } from "@testing-library/jest-dom/matchers";

declare module "vitest" {
  // Declaration merging: the type parameters must match Vitest's exactly, and the body
  // stays empty because the interface only adds a supertype.
  // eslint-disable-next-line @typescript-eslint/no-empty-object-type
  interface Matchers<
    R extends void | Promise<void> = void | Promise<void>,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    T = unknown,
  > extends TestingLibraryMatchers<unknown, R> {}
}
