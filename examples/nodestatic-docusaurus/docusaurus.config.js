const config = {
  title: "Docusaurus Example",
  tagline: "Static docs built by Anybuild",
  favicon: "img/favicon.ico",
  url: "https://example.com",
  baseUrl: "/",
  organizationName: "anybuild",
  projectName: "nodestatic-docusaurus",
  trailingSlash: false,
  presets: [
    [
      "classic",
      {
        docs: {
          sidebarPath: require.resolve("./sidebars.js"),
        },
        blog: false,
        theme: {
          customCss: require.resolve("./src/css/custom.css"),
        },
      },
    ],
  ],
};

module.exports = config;
