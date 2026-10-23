// Where the AI service is (ADR-0037): learned from the build configuration,
// `VITE_AI_SERVICE_URL`. A build without one has no AI, and nothing else
// changes; the AI panel says so (#260).

/** The service's base URL, or `undefined` when the build has none or it is not one. */
export function aiServiceUrl(configured: string | undefined): URL | undefined {
  const text = configured?.trim();
  if (!text) return undefined;
  let url: URL;
  try {
    url = new URL(text);
  } catch {
    return undefined;
  }
  // https everywhere but on the developer's own machine.
  const local = url.hostname === "localhost" || url.hostname === "127.0.0.1";
  if (url.protocol !== "https:" && !(url.protocol === "http:" && local)) return undefined;
  if (url.username || url.password || url.search || url.hash) return undefined;
  return url;
}

/** This build's AI service, if it was built with one. */
export const AI_SERVICE_URL: URL | undefined = aiServiceUrl(
  import.meta.env.VITE_AI_SERVICE_URL as string | undefined,
);
