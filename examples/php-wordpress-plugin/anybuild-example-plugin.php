<?php
/**
 * Plugin Name: Anybuild Example Plugin
 * Description: Demonstrates a WordPress plugin deployed by Anybuild.
 * Version: 1.0.0
 * Author: Wasmer
 */

add_action('wp_footer', function () {
    echo '<div id="anybuild-example-plugin">Anybuild WordPress plugin active</div>';
});
