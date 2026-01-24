import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:gosh_distrobox_manager/providers/app_state.dart';
import 'package:gosh_distrobox_manager/src/rust/backends/distrobox/distrobox.dart';

class PackageManagerPage extends StatefulWidget {
  const PackageManagerPage({super.key});

  @override
  State<PackageManagerPage> createState() => _PackageManagerPageState();
}

class _PackageManagerPageState extends State<PackageManagerPage>
    with SingleTickerProviderStateMixin {
  ContainerInfo? _selectedContainer;
  final TextEditingController _searchController = TextEditingController();
  late TabController _tabController;
  bool _isSearching = false;

  @override
  void initState() {
    super.initState();
    _tabController = TabController(length: 2, vsync: this);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      final appState = context.read<AppStateProvider>();
      appState.refresh();
      // Select first running container if available
      final runningContainers =
          appState.containers.where((c) => c.status is Status_Up).toList();
      if (runningContainers.isNotEmpty) {
        _selectContainer(runningContainers.first);
      } else if (appState.containers.isNotEmpty) {
        _selectContainer(appState.containers.first);
      }
    });
  }

  void _selectContainer(ContainerInfo container) {
    setState(() {
      _selectedContainer = container;
    });
    final appState = context.read<AppStateProvider>();
    // Detect package manager and load installed packages
    appState.detectPackageManager(container.name);
    if (container.status is Status_Up) {
      appState.loadInstalledPackages(container.name);
    }
  }

  @override
  void dispose() {
    _searchController.dispose();
    _tabController.dispose();
    super.dispose();
  }

  IconData _getDistroIcon(String image) {
    final imageLower = image.toLowerCase();
    if (imageLower.contains('ubuntu')) return Icons.circle;
    if (imageLower.contains('fedora')) return Icons.filter_vintage;
    if (imageLower.contains('arch')) return Icons.architecture;
    if (imageLower.contains('debian')) return Icons.donut_large;
    if (imageLower.contains('alpine')) return Icons.landscape;
    return Icons.dns;
  }

  void _onSearchSubmitted(String query) {
    if (_selectedContainer == null || query.trim().isEmpty) return;

    final appState = context.read<AppStateProvider>();
    appState.searchPackages(_selectedContainer!.name, query.trim());
    setState(() => _isSearching = true);
    _tabController.animateTo(1); // Switch to search results tab
  }

  void _clearSearch() {
    _searchController.clear();
    context.read<AppStateProvider>().clearSearchResults();
    setState(() => _isSearching = false);
    _tabController.animateTo(0); // Switch back to installed tab
  }

  Future<void> _installPackage(String packageName) async {
    if (_selectedContainer == null) return;

    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Install Package'),
        content: Text('Install "$packageName" in "${_selectedContainer!.name}"?'),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Install'),
          ),
        ],
      ),
    );

    if (confirmed == true && mounted) {
      final appState = context.read<AppStateProvider>();
      final taskId = await appState.installPackage(
        _selectedContainer!.name,
        packageName,
      );
      if (taskId != null && mounted) {
        _showTaskProgressDialog(taskId, 'Installing $packageName');
      }
    }
  }

  Future<void> _removePackage(String packageName) async {
    if (_selectedContainer == null) return;

    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Remove Package'),
        content: Text(
          'Remove "$packageName" from "${_selectedContainer!.name}"?\n\n'
          'This may also remove dependent packages.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            style: FilledButton.styleFrom(backgroundColor: Colors.red),
            child: const Text('Remove'),
          ),
        ],
      ),
    );

    if (confirmed == true && mounted) {
      final appState = context.read<AppStateProvider>();
      final taskId = await appState.removePackage(
        _selectedContainer!.name,
        packageName,
      );
      if (taskId != null && mounted) {
        _showTaskProgressDialog(taskId, 'Removing $packageName');
      }
    }
  }

  void _showTaskProgressDialog(String taskId, String title) {
    showDialog(
      context: context,
      barrierDismissible: false,
      builder: (context) => _TaskProgressDialog(
        taskId: taskId,
        title: title,
        onComplete: () {
          // Refresh package list after operation
          if (_selectedContainer != null) {
            context.read<AppStateProvider>().loadInstalledPackages(
              _selectedContainer!.name,
            );
          }
        },
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Package Manager'),
        actions: [
          IconButton(
            icon: const Icon(Icons.refresh),
            onPressed: () {
              final appState = context.read<AppStateProvider>();
              appState.refresh();
              if (_selectedContainer != null &&
                  _selectedContainer!.status is Status_Up) {
                appState.loadInstalledPackages(_selectedContainer!.name);
              }
            },
          ),
        ],
        bottom: TabBar(
          controller: _tabController,
          tabs: [
            const Tab(text: 'Installed'),
            Tab(text: _isSearching ? 'Search Results' : 'Search'),
          ],
        ),
      ),
      body: Consumer<AppStateProvider>(
        builder: (context, appState, child) {
          if (appState.isLoading) {
            return const Center(child: CircularProgressIndicator());
          }

          if (!appState.isDistroboxInstalled) {
            return _buildNotInstalledView(context);
          }

          if (appState.containers.isEmpty) {
            return _buildNoContainersView(context);
          }

          // Update selected container if needed
          if (_selectedContainer != null &&
              !appState.containers.any((c) => c.name == _selectedContainer!.name)) {
            _selectedContainer = appState.containers.first;
          } else if (_selectedContainer == null) {
            _selectContainer(appState.containers.first);
          }

          final isRunning = _selectedContainer?.status is Status_Up;
          final packageManager = appState.detectedPackageManager ?? 'Detecting...';

          return Column(
            children: [
              // Container Selector
              Padding(
                padding: const EdgeInsets.all(16),
                child: _buildContainerSelector(context, appState),
              ),

              // Status banner for stopped containers
              if (!isRunning)
                Container(
                  margin: const EdgeInsets.symmetric(horizontal: 16),
                  padding: const EdgeInsets.all(12),
                  decoration: BoxDecoration(
                    color: Colors.orange.withOpacity(0.1),
                    borderRadius: BorderRadius.circular(12),
                    border: Border.all(color: Colors.orange.withOpacity(0.3)),
                  ),
                  child: Row(
                    children: [
                      const Icon(Icons.warning_amber, color: Colors.orange),
                      const SizedBox(width: 12),
                      Expanded(
                        child: Text(
                          'Container is not running. Start it to manage packages.',
                          style: TextStyle(color: Colors.orange[800]),
                        ),
                      ),
                    ],
                  ),
                ),

              // Search Bar
              Padding(
                padding: const EdgeInsets.all(16.0),
                child: SearchBar(
                  controller: _searchController,
                  leading: const Icon(Icons.search),
                  hintText: 'Search for packages...',
                  onSubmitted: isRunning ? _onSearchSubmitted : null,
                  elevation: WidgetStateProperty.all(0),
                  backgroundColor: WidgetStateProperty.resolveWith((states) {
                    return Theme.of(context).brightness == Brightness.dark
                        ? Colors.grey[800]
                        : Colors.white;
                  }),
                  shape: WidgetStateProperty.all(RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(12),
                    side: BorderSide(color: Theme.of(context).dividerColor),
                  )),
                  trailing: [
                    if (_isSearching)
                      IconButton(
                        icon: const Icon(Icons.close),
                        onPressed: _clearSearch,
                      ),
                    Container(
                      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                      decoration: BoxDecoration(
                        color: Theme.of(context).colorScheme.primary.withOpacity(0.1),
                        borderRadius: BorderRadius.circular(6),
                      ),
                      child: Text(
                        packageManager.toUpperCase(),
                        style: TextStyle(
                          color: Theme.of(context).colorScheme.primary,
                          fontWeight: FontWeight.bold,
                          fontSize: 10,
                        ),
                      ),
                    ),
                    const SizedBox(width: 8),
                  ],
                ),
              ),

              // Quick Actions
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 16.0),
                child: Row(
                  children: [
                    Expanded(
                      child: OutlinedButton.icon(
                        onPressed: isRunning
                            ? () => _installPackage(_searchController.text.trim())
                            : null,
                        icon: const Icon(Icons.add),
                        label: const Text('Install'),
                        style: OutlinedButton.styleFrom(
                          padding: const EdgeInsets.symmetric(vertical: 12),
                          shape: RoundedRectangleBorder(
                              borderRadius: BorderRadius.circular(12)),
                        ),
                      ),
                    ),
                    const SizedBox(width: 12),
                    Expanded(
                      child: FilledButton.icon(
                        onPressed: isRunning
                            ? () => _showUpgradeDialog(context, appState)
                            : null,
                        icon: const Icon(Icons.system_update_alt),
                        label: const Text('Upgrade All'),
                        style: FilledButton.styleFrom(
                          padding: const EdgeInsets.symmetric(vertical: 12),
                          shape: RoundedRectangleBorder(
                              borderRadius: BorderRadius.circular(12)),
                        ),
                      ),
                    ),
                  ],
                ),
              ),

              const SizedBox(height: 16),

              // Tab Content
              Expanded(
                child: TabBarView(
                  controller: _tabController,
                  children: [
                    _buildInstalledPackagesList(context, appState, isRunning),
                    _buildSearchResultsList(context, appState, isRunning),
                  ],
                ),
              ),
            ],
          );
        },
      ),
    );
  }

  Widget _buildContainerSelector(BuildContext context, AppStateProvider appState) {
    return Card(
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(color: Theme.of(context).dividerColor),
      ),
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Row(
          children: [
            Container(
              width: 40,
              height: 40,
              decoration: BoxDecoration(
                color: Theme.of(context).colorScheme.primary.withOpacity(0.1),
                borderRadius: BorderRadius.circular(10),
              ),
              child: Icon(
                _selectedContainer != null
                    ? _getDistroIcon(_selectedContainer!.image)
                    : Icons.dns,
                color: Theme.of(context).colorScheme.primary,
              ),
            ),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    'CONTAINER',
                    style: TextStyle(
                      fontSize: 10,
                      fontWeight: FontWeight.bold,
                      color: Theme.of(context).hintColor,
                      letterSpacing: 0.5,
                    ),
                  ),
                  DropdownButton<String>(
                    value: _selectedContainer?.name,
                    icon: const Icon(Icons.expand_more),
                    underline: const SizedBox(),
                    isDense: true,
                    style: TextStyle(
                      fontSize: 16,
                      fontWeight: FontWeight.bold,
                      color: Theme.of(context).textTheme.bodyLarge?.color,
                    ),
                    onChanged: (String? newValue) {
                      if (newValue != null) {
                        final container = appState.containers
                            .firstWhere((c) => c.name == newValue);
                        _selectContainer(container);
                      }
                    },
                    items: appState.containers.map((container) {
                      final isRunning = container.status is Status_Up;
                      return DropdownMenuItem<String>(
                        value: container.name,
                        child: Row(
                          children: [
                            Text(container.name),
                            const SizedBox(width: 8),
                            Container(
                              width: 8,
                              height: 8,
                              decoration: BoxDecoration(
                                color: isRunning ? Colors.green : Colors.grey,
                                shape: BoxShape.circle,
                              ),
                            ),
                          ],
                        ),
                      );
                    }).toList(),
                  ),
                ],
              ),
            ),
            if (_selectedContainer != null)
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
                decoration: BoxDecoration(
                  color: _selectedContainer!.status is Status_Up
                      ? Colors.green.withOpacity(0.1)
                      : Colors.grey.withOpacity(0.1),
                  borderRadius: BorderRadius.circular(8),
                ),
                child: Text(
                  _selectedContainer!.status is Status_Up ? 'Running' : 'Stopped',
                  style: TextStyle(
                    fontSize: 12,
                    fontWeight: FontWeight.bold,
                    color: _selectedContainer!.status is Status_Up
                        ? Colors.green
                        : Colors.grey,
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }

  Widget _buildInstalledPackagesList(
      BuildContext context, AppStateProvider appState, bool isRunning) {
    if (!isRunning) {
      return Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(Icons.power_off, size: 64, color: Theme.of(context).hintColor),
            const SizedBox(height: 16),
            const Text('Container Not Running'),
            const SizedBox(height: 8),
            Text(
              'Start the container to view installed packages',
              style: TextStyle(color: Theme.of(context).hintColor),
            ),
          ],
        ),
      );
    }

    if (appState.isLoadingPackages) {
      return const Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            CircularProgressIndicator(),
            SizedBox(height: 16),
            Text('Loading packages...'),
          ],
        ),
      );
    }

    if (appState.packagesError != null) {
      return Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            const Icon(Icons.error_outline, size: 64, color: Colors.red),
            const SizedBox(height: 16),
            Text(appState.packagesError!),
            const SizedBox(height: 16),
            ElevatedButton(
              onPressed: () {
                if (_selectedContainer != null) {
                  appState.loadInstalledPackages(_selectedContainer!.name);
                }
              },
              child: const Text('Retry'),
            ),
          ],
        ),
      );
    }

    final packages = appState.installedPackages;

    if (packages.isEmpty) {
      return Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(Icons.inventory_2_outlined, size: 64, color: Theme.of(context).hintColor),
            const SizedBox(height: 16),
            const Text('No packages found'),
          ],
        ),
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: Text(
            '${packages.length} packages installed',
            style: TextStyle(
              fontSize: 12,
              color: Theme.of(context).hintColor,
            ),
          ),
        ),
        const SizedBox(height: 8),
        Expanded(
          child: ListView.builder(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            itemCount: packages.length,
            itemBuilder: (context, index) {
              final pkg = packages[index];
              return _buildPackageCard(
                context,
                pkg.name,
                pkg.version,
                pkg.description,
                isInstalled: true,
                onAction: () => _removePackage(pkg.name),
              );
            },
          ),
        ),
      ],
    );
  }

  Widget _buildSearchResultsList(
      BuildContext context, AppStateProvider appState, bool isRunning) {
    if (!_isSearching) {
      return Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(Icons.search, size: 64, color: Theme.of(context).hintColor),
            const SizedBox(height: 16),
            const Text('Search for packages'),
            const SizedBox(height: 8),
            Text(
              'Enter a package name and press Enter',
              style: TextStyle(color: Theme.of(context).hintColor),
            ),
          ],
        ),
      );
    }

    if (appState.isLoadingPackages) {
      return const Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            CircularProgressIndicator(),
            SizedBox(height: 16),
            Text('Searching...'),
          ],
        ),
      );
    }

    final results = appState.searchResults;

    if (results.isEmpty) {
      return Center(
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(Icons.search_off, size: 64, color: Theme.of(context).hintColor),
            const SizedBox(height: 16),
            const Text('No packages found'),
            const SizedBox(height: 8),
            Text(
              'Try a different search term',
              style: TextStyle(color: Theme.of(context).hintColor),
            ),
          ],
        ),
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: Text(
            '${results.length} packages found',
            style: TextStyle(
              fontSize: 12,
              color: Theme.of(context).hintColor,
            ),
          ),
        ),
        const SizedBox(height: 8),
        Expanded(
          child: ListView.builder(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            itemCount: results.length,
            itemBuilder: (context, index) {
              final pkg = results[index];
              return _buildPackageCard(
                context,
                pkg.name,
                pkg.version,
                pkg.description,
                isInstalled: false,
                onAction: () => _installPackage(pkg.name),
              );
            },
          ),
        ),
      ],
    );
  }

  Widget _buildPackageCard(
    BuildContext context,
    String name,
    String version,
    String description, {
    required bool isInstalled,
    required VoidCallback onAction,
  }) {
    return Card(
      margin: const EdgeInsets.only(bottom: 8),
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(color: Theme.of(context).dividerColor),
      ),
      child: ListTile(
        contentPadding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
        leading: Container(
          width: 40,
          height: 40,
          decoration: BoxDecoration(
            color: isInstalled
                ? Colors.green.withOpacity(0.1)
                : Theme.of(context).colorScheme.primary.withOpacity(0.1),
            borderRadius: BorderRadius.circular(8),
          ),
          child: Icon(
            isInstalled ? Icons.check_circle : Icons.download,
            color: isInstalled ? Colors.green : Theme.of(context).colorScheme.primary,
          ),
        ),
        title: Row(
          children: [
            Expanded(
              child: Text(
                name,
                style: const TextStyle(fontWeight: FontWeight.bold),
                overflow: TextOverflow.ellipsis,
              ),
            ),
            if (version.isNotEmpty)
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                decoration: BoxDecoration(
                  color: Theme.of(context).hintColor.withOpacity(0.1),
                  borderRadius: BorderRadius.circular(4),
                ),
                child: Text(
                  version,
                  style: TextStyle(
                    fontSize: 10,
                    color: Theme.of(context).hintColor,
                    fontFamily: 'monospace',
                  ),
                ),
              ),
          ],
        ),
        subtitle: description.isNotEmpty
            ? Text(
                description,
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontSize: 12,
                  color: Theme.of(context).hintColor,
                ),
              )
            : null,
        trailing: IconButton(
          icon: Icon(
            isInstalled ? Icons.delete_outline : Icons.add,
            color: isInstalled ? Colors.red : Theme.of(context).colorScheme.primary,
          ),
          onPressed: onAction,
          tooltip: isInstalled ? 'Remove' : 'Install',
        ),
      ),
    );
  }

  void _showUpgradeDialog(BuildContext context, AppStateProvider appState) async {
    if (_selectedContainer == null) return;

    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Upgrade All Packages'),
        content: Text(
          'Upgrade all packages in "${_selectedContainer!.name}"?\n\n'
          'This may take several minutes.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Upgrade'),
          ),
        ],
      ),
    );

    if (confirmed == true && mounted) {
      final taskId = await appState.upgradeContainer(_selectedContainer!.name);
      if (taskId != null && mounted) {
        _showTaskProgressDialog(taskId, 'Upgrading ${_selectedContainer!.name}');
      }
    }
  }

  Widget _buildNotInstalledView(BuildContext context) {
    return Center(
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          const Icon(Icons.warning_amber_rounded, size: 64, color: Colors.orange),
          const SizedBox(height: 16),
          const Text('Distrobox Not Found',
              style: TextStyle(fontSize: 24, fontWeight: FontWeight.bold)),
          const SizedBox(height: 8),
          const Text('Please install Distrobox to use this feature.'),
        ],
      ),
    );
  }

  Widget _buildNoContainersView(BuildContext context) {
    return Center(
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Icon(Icons.inventory_2_outlined,
              size: 64, color: Theme.of(context).hintColor),
          const SizedBox(height: 16),
          const Text('No Containers',
              style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold)),
          const SizedBox(height: 8),
          Text('Create a container to manage packages.',
              style: TextStyle(color: Theme.of(context).hintColor)),
        ],
      ),
    );
  }
}

/// Dialog showing task progress with streaming output
class _TaskProgressDialog extends StatefulWidget {
  final String taskId;
  final String title;
  final VoidCallback? onComplete;

  const _TaskProgressDialog({
    required this.taskId,
    required this.title,
    this.onComplete,
  });

  @override
  State<_TaskProgressDialog> createState() => _TaskProgressDialogState();
}

class _TaskProgressDialogState extends State<_TaskProgressDialog> {
  final ScrollController _scrollController = ScrollController();
  bool _isComplete = false;

  @override
  void initState() {
    super.initState();
    _checkCompletion();
  }

  void _checkCompletion() async {
    while (mounted && !_isComplete) {
      await Future.delayed(const Duration(milliseconds: 500));
      if (!mounted) return;
      
      final appState = context.read<AppStateProvider>();
      final isRunning = await appState.isTaskRunning(widget.taskId);
      
      if (!isRunning) {
        setState(() => _isComplete = true);
        widget.onComplete?.call();
      }
    }
  }

  @override
  void dispose() {
    _scrollController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: Row(
        children: [
          if (!_isComplete) ...[
            const SizedBox(
              width: 20,
              height: 20,
              child: CircularProgressIndicator(strokeWidth: 2),
            ),
            const SizedBox(width: 12),
          ] else ...[
            const Icon(Icons.check_circle, color: Colors.green, size: 24),
            const SizedBox(width: 12),
          ],
          Expanded(child: Text(widget.title)),
        ],
      ),
      content: SizedBox(
        width: double.maxFinite,
        height: 300,
        child: Consumer<AppStateProvider>(
          builder: (context, appState, child) {
            final task = appState.getTask(widget.taskId);
            final output = task?.output ?? [];

            WidgetsBinding.instance.addPostFrameCallback((_) {
              if (_scrollController.hasClients) {
                _scrollController.animateTo(
                  _scrollController.position.maxScrollExtent,
                  duration: const Duration(milliseconds: 100),
                  curve: Curves.easeOut,
                );
              }
            });

            return Container(
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: Colors.black87,
                borderRadius: BorderRadius.circular(8),
              ),
              child: ListView.builder(
                controller: _scrollController,
                itemCount: output.length,
                itemBuilder: (context, index) {
                  return Text(
                    output[index],
                    style: const TextStyle(
                      fontFamily: 'monospace',
                      fontSize: 11,
                      color: Colors.white,
                    ),
                  );
                },
              ),
            );
          },
        ),
      ),
      actions: [
        if (_isComplete)
          FilledButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Done'),
          )
        else
          TextButton(
            onPressed: () {
              context.read<AppStateProvider>().cancelTask(widget.taskId);
              Navigator.pop(context);
            },
            child: const Text('Cancel'),
          ),
      ],
    );
  }
}
