// Dumb CORS relay for DJI's keychain decryption API. No secrets, no logic
// beyond forwarding the request and adding CORS headers scoped to the
// GitHub Pages origin. The caller's API key travels through unmodified —
// this worker never inspects or stores it.
//
// Restricting by the Origin header is a convenience gate, not real access
// control (it's spoofable with curl) — that's fine here, since this is
// explicitly a dumb relay with no secrets of its own, no worse than the
// bundled default API key already being effectively public.

const DJI_KEYCHAINS_URL = "https://dev.dji.com/openapi/v1/flight-records/keychains";
const ALLOWED_ORIGIN = "https://fractalated.github.io";

function corsHeaders() {
  return {
    "Access-Control-Allow-Origin": ALLOWED_ORIGIN,
    "Access-Control-Allow-Methods": "POST, OPTIONS",
    "Access-Control-Allow-Headers": "Content-Type, Api-Key",
    "Access-Control-Max-Age": "86400",
  };
}

export default {
  async fetch(request) {
    if (request.method === "OPTIONS") {
      return new Response(null, { status: 204, headers: corsHeaders() });
    }

    if (request.method !== "POST") {
      return new Response("Method not allowed", {
        status: 405,
        headers: corsHeaders(),
      });
    }

    const body = await request.text();

    const upstream = await fetch(DJI_KEYCHAINS_URL, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "Api-Key": request.headers.get("Api-Key") ?? "",
      },
      body,
    });

    const responseBody = await upstream.text();
    const headers = new Headers(corsHeaders());
    headers.set(
      "Content-Type",
      upstream.headers.get("Content-Type") ?? "application/json"
    );

    return new Response(responseBody, { status: upstream.status, headers });
  },
};
