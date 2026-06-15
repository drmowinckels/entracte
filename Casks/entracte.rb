cask "entracte" do
  arch arm: "aarch64", intel: "x64"

  version "0.0.7"
  sha256 arm:   "a2de148c1fcdb5c8d4a6305cabdf344c6951bbd9c3d986e026d75d20817521f3",
         intel: "4d63529d0cad15ec4089d9c46fc6a1d04ced42609adb6ff132915beef7f3cc1b"

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

  zap trash: [
    "~/Library/Application Support/io.drmowinckels.entracte",
    "~/Library/Caches/io.drmowinckels.entracte",
    "~/Library/Logs/io.drmowinckels.entracte",
    "~/Library/Preferences/io.drmowinckels.entracte.plist",
    "~/Library/LaunchAgents/io.drmowinckels.entracte.plist",
    "~/Library/Saved Application State/io.drmowinckels.entracte.savedState",
  ]
end
