cask "twin" do
  version "0.1.5"
  sha256 :no_check

  url "https://github.com/1-800-CASUALME/twin/releases/download/v#{version}/Twin-macos.dmg"
  name "Twin"
  desc "Keep two workstations in sync: Connect, Diagnose, Sync"
  homepage "https://github.com/1-800-CASUALME/twin"

  depends_on macos: ">= :sequoia"

  app "Twin.app"
  binary "#{appdir}/Twin.app/Contents/MacOS/twin-cli", target: "twin"

  zap trash: ["~/.twin", "~/Library/LaunchAgents/com.asim.twin.sync.plist"]
end
