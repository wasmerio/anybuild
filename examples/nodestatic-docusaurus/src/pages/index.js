import Link from "@docusaurus/Link";
import Layout from "@theme/Layout";

export default function Home() {
  return (
    <Layout
      title="Docusaurus Example"
      description="Static Docusaurus example built by Shipit"
    >
      <main style={{ padding: "4rem 2rem", textAlign: "center" }}>
        <h1>Docusaurus Example</h1>
        <p>Static docs built and served by Shipit.</p>
        <Link to="/docs/intro">Read the intro</Link>
      </main>
    </Layout>
  );
}
