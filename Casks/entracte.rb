cask "entracte" do
  arch arm: "aarch64", intel: "x64"

  version "0.0.9"
  sha256 arm:   "ce05944b60d2c2d8e4fa4f15bee2a2d1ab403e9747068ed3061bd6d9e8cd6155",
         intel: "6fa63287d54f24ec4076057ddafb41c90741bbba2117bdfe29137588f98e70a1"

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
