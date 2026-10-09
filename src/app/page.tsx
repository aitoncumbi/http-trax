"use client";

import dynamic from "next/dynamic";

// The app is entirely client-side (it talks to Tauri), so skip prerendering.
const App = dynamic(() => import("@/components/App"), {
  ssr: false,
  loading: () => (
    <div className="flex h-screen items-center justify-center text-sm text-faint">Loading http-trax…</div>
  ),
});

export default function Page() {
  return <App />;
}
