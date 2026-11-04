<?php

declare(strict_types=1);

use Twig\Environment;
use Twig\Loader\FilesystemLoader;

$projectRoot = dirname(__DIR__);
$autoloadPath = $projectRoot . '/vendor/autoload.php';

if (!is_file($autoloadPath)) {
    throw new RuntimeException('Twig is not installed. Run composer install from the project root.');
}

require $autoloadPath;

$loader = new FilesystemLoader($projectRoot . '/ui');
$twig = new Environment($loader, [
    'autoescape' => 'html',
    'strict_variables' => true,
]);

$outputDirectory = $projectRoot . '/ui/build';
foreach ([$outputDirectory, $outputDirectory . '/css', $outputDirectory . '/js'] as $directory) {
    if (!is_dir($directory) && !mkdir($directory, 0775, true) && !is_dir($directory)) {
        throw new RuntimeException(sprintf('Unable to create frontend output directory: %s', $directory));
    }
}

$html = $twig->render('index.twig', [
    'description' => 'A clear, safety-first desktop workflow for rsync over SSH.',
    'page_title' => 'Rsync Desktop',
]);

$htmlPath = $outputDirectory . '/index.html';
if (file_put_contents($htmlPath, $html) === false) {
    throw new RuntimeException(sprintf('Unable to write rendered frontend: %s', $htmlPath));
}

foreach (['css/app.css', 'js/app.js'] as $asset) {
    $source = $projectRoot . '/ui/' . $asset;
    $destination = $outputDirectory . '/' . $asset;
    if (!copy($source, $destination)) {
        throw new RuntimeException(sprintf('Unable to copy frontend asset: %s', $source));
    }
}

fwrite(STDOUT, "Rendered Twig frontend to ui/build/index.html\n");
