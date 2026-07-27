export const metadata = {
  title: "Anybuild Next.js Example",
  description: "A Next.js runtime app built by Anybuild",
};

export default function RootLayout({ children }) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
