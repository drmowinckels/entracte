cask "entracte" do
  arch arm: "aarch64", intel: "x64"

  version "0.0.14"
  sha256 arm:   "fec4d9b1a4bb965ac46201cd2a7157341591b214eb409be7e91041a6b1e09ac3",
         intel: "5ae142ede7ad52c0203bc17c8cc3253b54d29bf0d2b2e9f5f6373471c7a4de5c"

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
