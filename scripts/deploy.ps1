<#
.SYNOPSIS
    Vérifie le projet, puis l'enregistre et l'envoie sur GitHub.

.DESCRIPTION
    1. Initialise git et relie le dépôt GitHub si ce n'est pas déjà fait.
    2. Lance les mêmes vérifications que l'intégration continue (format, clippy, tests).
    3. Montre les fichiers modifiés, demande confirmation, crée un commit et le pousse.

    Le script ne force jamais un envoi (pas de « push --force ») et refuse d'envoyer un
    fichier qui ressemble à un secret (.env, clé, keystore…).

.PARAMETER Message
    Message du commit. S'il est absent, le script le demande.

.PARAMETER SkipChecks
    Saute les vérifications (à éviter : l'intégration continue les relancera de toute façon).

.PARAMETER Yes
    Ne demande pas de confirmation avant d'envoyer.

.EXAMPLE
    .\deploy.cmd "feat: écran Aujourd'hui"

.EXAMPLE
    .\deploy.cmd -SkipChecks "docs: mise à jour du cahier des charges"
#>
[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [string]$Message,
    [switch]$SkipChecks,
    [switch]$Yes
)

$ErrorActionPreference = 'Stop'
$RemoteUrl = 'https://github.com/MoreIIo/Fuck-Calorie.git'
$RepoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $RepoRoot

function Step([string]$Text) {
    Write-Host ''
    Write-Host "==> $Text" -ForegroundColor Cyan
}

function Fail([string]$Text) {
    Write-Host ''
    Write-Host "ERREUR : $Text" -ForegroundColor Red
    exit 1
}

# À appeler juste après une commande externe (git, cargo) : arrête tout si elle a échoué.
function Assert-Ok([string]$What) {
    if ($LASTEXITCODE -ne 0) {
        Fail "$What a échoué. Rien n'a été envoyé."
    }
}

# ----- Outils -----------------------------------------------------------------------------

foreach ($tool in 'git', 'cargo') {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        Fail "« $tool » est introuvable. Installe-le, rouvre le terminal, puis relance le script."
    }
}

# ----- Dépôt git --------------------------------------------------------------------------

Step 'Dépôt git'
if (-not (Test-Path (Join-Path $RepoRoot '.git'))) {
    git init -b main
    Assert-Ok 'git init'
}
if (-not ((git remote) -contains 'origin')) {
    git remote add origin $RemoteUrl
    Assert-Ok 'git remote add'
    Write-Host "Dépôt distant relié : $RemoteUrl"
}

$userName = git config user.name
$userEmail = git config user.email
if (-not $userName -or -not $userEmail) {
    Fail ("Git ne connaît pas encore ton identité. Lance ces deux commandes, puis relance le script :`n" +
        "  git config --global user.name `"Ton nom`"`n" +
        "  git config --global user.email `"ton adresse e-mail`"")
}

$branch = git symbolic-ref --short HEAD
Assert-Ok 'La lecture de la branche courante'
Write-Host "Branche : $branch"

# ----- Vérifications ----------------------------------------------------------------------

if ($SkipChecks) {
    Step 'Vérifications sautées (-SkipChecks)'
}
else {
    Step 'Format'
    cargo fmt --all -- --check
    Assert-Ok 'La vérification du format (lance « cargo fmt --all » pour corriger)'

    Step 'Clippy'
    cargo clippy --workspace --all-targets -- -D warnings
    Assert-Ok 'Clippy'

    Step 'Tests'
    cargo test --workspace
    Assert-Ok 'Les tests'
}

# ----- Commit -----------------------------------------------------------------------------

Step 'Modifications'
git add -A
Assert-Ok 'git add'

$staged = @(git diff --cached --name-only)
$secrets = @($staged | Where-Object { $_ -match '(^|/)\.env(\.|$)|\.(pem|p12|pfx|key|keystore|jks)$' })
if ($secrets.Count -gt 0) {
    git reset --quiet
    Fail ("Ces fichiers ressemblent à des secrets et ne doivent pas aller sur GitHub :`n  " +
        ($secrets -join "`n  ") +
        "`nAjoute-les à .gitignore (ou supprime-les), puis relance le script.")
}

if ($staged.Count -gt 0) {
    git status --short
    if (-not $Message) {
        $Message = Read-Host 'Message du commit (ex. feat: ajout rapide)'
    }
    if (-not $Message -or -not $Message.Trim()) {
        Fail 'Message de commit vide.'
    }
}
else {
    Write-Host 'Aucune modification à enregistrer.'
}

if (-not $Yes) {
    $answer = Read-Host "Envoyer sur GitHub (branche $branch) ? [o/N]"
    if ($answer -notmatch '^[oOyY]') {
        Write-Host 'Annulé : rien n''a été enregistré ni envoyé.'
        exit 0
    }
}

if ($staged.Count -gt 0) {
    git commit -m $Message
    Assert-Ok 'git commit'
}

# ----- Envoi ------------------------------------------------------------------------------

Step "Envoi vers GitHub ($branch)"
git push -u origin $branch
if ($LASTEXITCODE -ne 0) {
    Fail ("L'envoi a échoué. Si GitHub contient des commits que tu n'as pas sur ce PC, lance`n" +
        "  git pull --rebase origin $branch`n" +
        "puis relance le script. Rien n'a été forcé.")
}

Write-Host ''
Write-Host 'Terminé : https://github.com/MoreIIo/Fuck-Calorie' -ForegroundColor Green
