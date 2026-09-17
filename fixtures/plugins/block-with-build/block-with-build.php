<?php
/**
 * Plugin Name:       Block With Build
 * Description:       A fixture plugin used by the SVNpush test suite.
 * Version:           0.3.0
 * Requires at least: 6.0
 * Requires PHP:      7.4
 * License:           GPL-2.0-or-later
 * License URI:       https://www.gnu.org/licenses/gpl-2.0.html
 * Text Domain:       block-with-build
 */

if ( ! defined( 'ABSPATH' ) ) {
	exit;
}

add_action( 'init', function () {
	register_block_type( __DIR__ . '/build' );
} );
