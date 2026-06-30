#!/usr/bin/env ruby
# Generates CarapaceWidgetSpike.xcodeproj (app + widget-extension targets) from the
# checked-in Swift sources. Run: ruby project.rb
# Reproducible substitute for the manual Xcode GUI steps in the spike plan.
require 'xcodeproj'
require 'fileutils'

ROOT = File.expand_path(File.dirname(__FILE__))           # .../widget-sample
PROJ = File.join(ROOT, 'CarapaceWidgetSpike.xcodeproj')
DEPLOY = '17.0'
GROUP_ID = 'group.carapace.spike'

FileUtils.rm_rf(PROJ)
project = Xcodeproj::Project.new(PROJ)

# ---------------------------------------------------------------- project-wide
project.build_configurations.each do |c|
  c.build_settings.merge!(
    'SDKROOT' => 'iphoneos',
    'IPHONEOS_DEPLOYMENT_TARGET' => DEPLOY,
    'SWIFT_VERSION' => '5.0',
    'MARKETING_VERSION' => '1.0',
    'CURRENT_PROJECT_VERSION' => '1',
    'CLANG_ENABLE_MODULES' => 'YES',
    'ALWAYS_SEARCH_USER_PATHS' => 'NO',
    'TARGETED_DEVICE_FAMILY' => '1,2',
    # Simulator: ad-hoc sign so App Group entitlements are honored, no team needed.
    # REQUIRED must be YES or Xcode skips entitlement processing and the App Group
    # container never resolves.
    'CODE_SIGN_STYLE' => 'Manual',
    'CODE_SIGN_IDENTITY' => '-',
    'CODE_SIGNING_REQUIRED' => 'YES',
    'CODE_SIGNING_ALLOWED' => 'YES',
    'DEVELOPMENT_TEAM' => ''
  )
  c.build_settings['ONLY_ACTIVE_ARCH'] = 'YES' if c.name == 'Debug'
end

# ---------------------------------------------------------------- helper groups
app_group    = project.main_group.new_group('App', 'App')
widget_group = project.main_group.new_group('Widget', 'Widget')
shared_group = project.main_group.new_group('Shared', 'Shared')
vendor_group = project.main_group.new_group('Vendor', 'Vendor')

shared_swift = shared_group.new_file(File.join(ROOT, 'Shared/AppGroup.swift'))

# ============================================================== APP TARGET =====
app = project.new_target(:application, 'CarapaceWidgetSpike', :ios, DEPLOY)

%w[App/CarapaceWidgetSpikeApp.swift App/ContentView.swift App/CarapaceBridge.swift].each do |rel|
  ref = app_group.new_file(File.join(ROOT, rel))
  app.add_file_references([ref])
end
app.add_file_references([shared_swift])

# Bridging header reference (not compiled, just present in the group).
app_group.new_file(File.join(ROOT, 'App/Bridging-Header.h'))
app_group.new_file(File.join(ROOT, 'App/CarapaceWidgetSpike.entitlements'))

# Skin as a folder reference (blue) -> copied wholesale into the app bundle as "skin-nowplaying"
# (the live-info skin: bound text + a seek bar rendered from host data).
skin_ref = project.main_group.new_file(File.join(ROOT, '..', 'skin-nowplaying'))
skin_ref.last_known_file_type = 'folder'
skin_ref.name = 'skin-nowplaying'
app.add_resources([skin_ref])

# Host-rendered fallback PNGs (Simulator can't run the live GPU render). Folder reference so
# they land at App.app/Seeded/state-*.png for seedFromBundle().
seeded_ref = app_group.new_file(File.join(ROOT, 'App/Seeded'))
seeded_ref.last_known_file_type = 'folder'
seeded_ref.name = 'Seeded'
app.add_resources([seeded_ref])

# Link + embed the Rust cdylib. We use the dylib (not the staticlib) because the
# staticlib leaks mlua's crate-private symbols as cross-object local references the
# system linker cannot resolve; rustc links the cdylib fully and exports only the
# public C ABI. The dylib's install_name is @rpath/libembed_spike.dylib.
dylib_ref = vendor_group.new_file(File.join(ROOT, 'Vendor/libembed_spike.dylib'))
app.frameworks_build_phase.add_file_reference(dylib_ref)
vendor_group.new_file(File.join(ROOT, 'Vendor/carapace.h'))
%w[Metal MetalKit QuartzCore CoreGraphics IOSurface].each { |fw| app.add_system_framework(fw) }

# Embed the dylib into App.app/Frameworks and code-sign it on copy.
embed_libs = app.new_copy_files_build_phase('Embed Libraries')
embed_libs.symbol_dst_subfolder_spec = :frameworks
elf = embed_libs.add_file_reference(dylib_ref)
elf.settings = { 'ATTRIBUTES' => ['CodeSignOnCopy'] }

app.build_configurations.each do |c|
  c.build_settings.merge!(
    'PRODUCT_BUNDLE_IDENTIFIER' => 'com.carapace.spike',
    'PRODUCT_NAME' => '$(TARGET_NAME)',
    'GENERATE_INFOPLIST_FILE' => 'YES',
    'INFOPLIST_KEY_UILaunchScreen_Generation' => 'YES',
    'INFOPLIST_KEY_UIApplicationSceneManifest_Generation' => 'YES',
    'SWIFT_OBJC_BRIDGING_HEADER' => 'App/Bridging-Header.h',
    'HEADER_SEARCH_PATHS' => '$(SRCROOT)/Vendor',
    'LIBRARY_SEARCH_PATHS' => '$(SRCROOT)/Vendor',
    'LD_RUNPATH_SEARCH_PATHS' => '$(inherited) @executable_path/Frameworks',
    'CODE_SIGN_ENTITLEMENTS' => 'App/CarapaceWidgetSpike.entitlements'
  )
end

# ============================================================== WIDGET TARGET ==
widget = project.new_target(:app_extension, 'CarapaceWidgetExtension', :ios, DEPLOY)

%w[Widget/CarapaceWidget.swift].each do |rel|
  ref = widget_group.new_file(File.join(ROOT, rel))
  widget.add_file_references([ref])
end
widget.add_file_references([shared_swift])
widget_group.new_file(File.join(ROOT, 'Widget/Info.plist'))
widget_group.new_file(File.join(ROOT, 'Widget/CarapaceWidgetExtension.entitlements'))

widget.build_configurations.each do |c|
  c.build_settings.merge!(
    'PRODUCT_BUNDLE_IDENTIFIER' => 'com.carapace.spike.widget',
    'PRODUCT_NAME' => '$(TARGET_NAME)',
    'GENERATE_INFOPLIST_FILE' => 'YES',
    'INFOPLIST_FILE' => 'Widget/Info.plist',
    'CODE_SIGN_ENTITLEMENTS' => 'Widget/CarapaceWidgetExtension.entitlements',
    'SKIP_INSTALL' => 'YES'
  )
end

# ---- Task 6 stretch probe: link carapace into the widget extension itself ----
# Enable with WIDGET_RENDER_PROBE=1 ruby project.rb. Lets Provider.entry() attempt a
# live render inside the extension process (see the probe edit in CarapaceWidget.swift).
if ENV['WIDGET_RENDER_PROBE'] == '1'
  widget.frameworks_build_phase.add_file_reference(dylib_ref)
  %w[Metal MetalKit QuartzCore CoreGraphics IOSurface].each { |fw| widget.add_system_framework(fw) }
  wembed = widget.new_copy_files_build_phase('Embed Libraries')
  wembed.symbol_dst_subfolder_spec = :frameworks
  wf = wembed.add_file_reference(dylib_ref)
  wf.settings = { 'ATTRIBUTES' => ['CodeSignOnCopy'] }
  widget.add_resources([skin_ref])
  widget.build_configurations.each do |c|
    c.build_settings.merge!(
      'SWIFT_OBJC_BRIDGING_HEADER' => 'App/Bridging-Header.h',
      'HEADER_SEARCH_PATHS' => '$(SRCROOT)/Vendor',
      'LIBRARY_SEARCH_PATHS' => '$(SRCROOT)/Vendor',
      'LD_RUNPATH_SEARCH_PATHS' => '$(inherited) @executable_path/Frameworks',
      'SWIFT_ACTIVE_COMPILATION_CONDITIONS' => '$(inherited) WIDGET_RENDER_PROBE'
    )
  end
end

# ---------------------------------------------------- embed widget into the app
app.add_dependency(widget)
embed = app.new_copy_files_build_phase('Embed Foundation Extensions')
embed.symbol_dst_subfolder_spec = :plug_ins
bf = embed.add_file_reference(widget.product_reference)
bf.settings = { 'ATTRIBUTES' => ['RemoveHeadersOnCopy'] }

# ----------------------------------------------------------------------- scheme
project.save

# Build a shared scheme so xcodebuild -scheme works headlessly.
scheme = Xcodeproj::XCScheme.new
scheme.add_build_target(app)
scheme.set_launch_target(app)
scheme.save_as(PROJ, 'CarapaceWidgetSpike', true)
puts "wrote #{PROJ}"
