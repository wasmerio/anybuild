<?php
/**
 * Main template for the Anybuild example theme.
 */
?><!doctype html>
<html <?php language_attributes(); ?>>
<head>
  <meta charset="<?php bloginfo('charset'); ?>">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <?php wp_head(); ?>
</head>
<body <?php body_class(); ?>>
  <?php wp_body_open(); ?>
  <main>
    <h1>Anybuild WordPress theme active</h1>
    <p>This page is rendered by the example theme.</p>
  </main>
  <?php wp_footer(); ?>
</body>
</html>
