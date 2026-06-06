cask "entracte" do
  arch arm: "aarch64", intel: "x64"

  version "0.0.6"
  sha256 arm:   "87d55a1994d8057a00a63f23204e9294861061ba05bf3fd6a0bf7154ae039cc7",
         intel: "7ae8447c25322fcf25360832e92559eeea1464a4fd51323fe37b8fa6ea0935d0"

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
