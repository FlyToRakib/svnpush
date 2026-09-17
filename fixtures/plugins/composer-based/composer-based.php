<?php
/**
 * Plugin Name:       Composer Based
 * Description:       A fixture plugin used by the SVNpush test suite.
 * Version:           1.4.2
 * Requires at least: 6.0
 * Requires PHP:      7.4
 * License:           GPL-2.0-or-later
 * License URI:       https://www.gnu.org/licenses/gpl-2.0.html
 * Text Domain:       composer-based
 */

if ( ! defined( 'ABSPATH' ) ) {
	exit;
}

define( 'COMPOSER_BASED_VERSION', '1.4.2' );

require_once __DIR__ . '/vendor/autoload.php';
