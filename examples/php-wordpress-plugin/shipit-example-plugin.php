<?php
/**
 * Plugin Name: Shipit Example Plugin
 * Description: Demonstrates a WordPress plugin deployed by Shipit.
 * Version: 1.0.0
 * Author: Wasmer
 */

add_action('wp_footer', function () {
    echo '<div id="shipit-example-plugin">Shipit WordPress plugin active</div>';
});
