import type { Metadata } from "next";

import "./globals.css";

export const metadata: Metadata = {
  title: "http-trax",
  description: "Git-native API client with scripting and an MCP server for AI agents",
};

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en" data-theme="dark" suppressHydrationWarning>
      <body>{children}</body>
    </html>
  );
}
