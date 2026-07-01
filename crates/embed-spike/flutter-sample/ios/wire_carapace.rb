#!/usr/bin/env ruby
# Wires the carapace cdylib + header + spike skin into the Flutter Runner target.
# Idempotent-ish: safe to re-run (skips refs that already exist by name).
# Run from flutter-sample/ios:  ruby wire_carapace.rb
require 'xcodeproj'

ROOT = File.expand_path(File.dirname(__FILE__))            # .../flutter-sample/ios
proj = Xcodeproj::Project.open(File.join(ROOT, 'Runner.xcodeproj'))
runner = proj.targets.find { |t| t.name == 'Runner' } or abort 'no Runner target'

main_group = proj.main_group
cara_group = main_group.find_subpath('Carapace', true)
cara_group.set_source_tree('SOURCE_ROOT')

# --- link + embed the cdylib --------------------------------------------------
dylib_path = 'Carapace/libembed_spike.dylib'
unless runner.frameworks_build_phase.files_references.any? { |r| r&.path == dylib_path }
  dylib_ref = cara_group.new_reference(dylib_path)
  dylib_ref.set_source_tree('SOURCE_ROOT')
  runner.frameworks_build_phase.add_file_reference(dylib_ref)
  # Embed into Runner.app/Frameworks, code-signed on copy.
  embed = runner.copy_files_build_phases.find { |p| p.name == 'Embed Carapace' } ||
          runner.new_copy_files_build_phase('Embed Carapace')
  embed.symbol_dst_subfolder_spec = :frameworks
  f = embed.add_file_reference(dylib_ref)
  f.settings = { 'ATTRIBUTES' => ['CodeSignOnCopy'] }
end

# --- header ref (not compiled, just visible/known to header search path) -------
unless cara_group.files.any? { |f| f.path == 'Carapace/carapace.h' }
  h = cara_group.new_reference('Carapace/carapace.h')
  h.set_source_tree('SOURCE_ROOT')
end

# --- bundle the skin folder(s) as blue folder references ----------------------
['skin-frame'].each do |name|
  next if main_group.files.any? { |f| f.name == name }
  ref = main_group.new_reference("../../#{name}")   # crates/embed-spike/#{name}
  ref.name = name
  ref.last_known_file_type = 'folder'
  runner.resources_build_phase.add_file_reference(ref)
end

# --- build settings: find carapace.h, find + rpath the dylib ------------------
runner.build_configurations.each do |c|
  bs = c.build_settings
  bs['HEADER_SEARCH_PATHS']  = ['$(inherited)', '$(SRCROOT)/Carapace']
  bs['LIBRARY_SEARCH_PATHS'] = ['$(inherited)', '$(SRCROOT)/Carapace']
  rp = Array(bs['LD_RUNPATH_SEARCH_PATHS'] || ['$(inherited)'])
  rp << '@executable_path/Frameworks' unless rp.include?('@executable_path/Frameworks')
  bs['LD_RUNPATH_SEARCH_PATHS'] = rp
end

# --- add CarapaceBridge.swift to the Runner compile sources -------------------
runner_group = main_group.find_subpath('Runner', false) || main_group
swift_rel = 'Runner/CarapaceBridge.swift'
unless runner.source_build_phase.files_references.any? { |r| r&.path&.end_with?('CarapaceBridge.swift') }
  sref = runner_group.new_reference('CarapaceBridge.swift')
  runner.source_build_phase.add_file_reference(sref)
end

proj.save
puts "wired carapace into Runner: link+embed dylib, header path, bundled skin, CarapaceBridge.swift"
