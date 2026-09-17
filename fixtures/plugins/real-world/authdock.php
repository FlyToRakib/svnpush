<?php
/**
 * AuthDock — Comprehensive Authentication & User Access Management
 *
 * @link              https://degird.com/
 * @since             1.0.0
 * @package           AuthDock
 *
 * @wordpress-plugin
 * Plugin Name:       AuthDock — Login Security, 2FA, Social Login & Brute Force Protection
 * Plugin URI:        https://degird.com/
 * Description:       A comprehensive authentication and user access management plugin for WordPress. Social login, magic link login, two-factor authentication, login attempt limiting, dynamic redirects, audit logging, wp-admin access restriction, and core security hardening — all with a native WordPress UI and REST API integration.
 * Version:           2.2.2
 * Author:            Degird
 * Author URI:        https://degird.com/
 * License:           GPL-2.0-or-later
 * License URI:       https://www.gnu.org/licenses/gpl-2.0.html
 * Text Domain:       authdock
 * Domain Path:       /languages
 * Requires at least: 6.0
 * Tested up to:      7.1
 * Requires PHP:      7.4
 */

// Prevent direct access.
if ( ! defined( 'ABSPATH' ) ) {
	exit;
}
