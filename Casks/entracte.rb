cask "entracte" do
  arch arm: "aarch64", intel: "x64"

  version "0.0.13"
  sha256 arm:   "a65131713ca0d1d44e50883c3106f1ae191f9e6ea45c2030c49cac748027da73",
         intel: "4a2db5cea1934794a53dc14b8b2afc4906a18d2896f9de3bb3c188dc009eaa8e"

  url "https://github.com/drmowinckels/entracte/releases/download/v#{version}/Entracte_#{version}_#{arch}.dmg",
      verified: "github.com/drmowinckels/entracte/"

  name "Entracte"
  desc "Cross-platform break reminder named after the theatre interval between acts"
  homepage "https://github.com/drmowinckels/entracte"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: ">= :big_sur"

  app "Entracte.app"
  binary "#{appdir}/Entracte.app/Contents/MacOS/entracte"

  zap trash: [
    "~/Library/Application Support/io.drmowinckels.entracte",
    "~/Library/Caches/io.drmowinckels.entracte",
    "~/Library/Logs/io.drmowinckels.entracte",
    "~/Library/Preferences/io.drmowinckels.entracte.plist",
    "~/Library/LaunchAgents/io.drmowinckels.entracte.plist",
    "~/Library/Saved Application State/io.drmowinckels.entracte.savedState",
  ]
end
