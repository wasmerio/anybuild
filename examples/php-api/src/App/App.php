<?php
namespace App;

use Bramus\Router\Router;

class App extends Router {
	public function __construct() {
		$this->setNamespace('\App\Controllers');
		
		$this->get('/', function() {
			header('Content-Type: application/json');
			echo json_encode([
				'env' => $_ENV,
				'get' => $_GET,
				'version' => phpversion(),
				'server' => $_SERVER
			]);
		});
		
		$this->get('/api/greet/(\w+)', 'Greeting@greet');
	}
}