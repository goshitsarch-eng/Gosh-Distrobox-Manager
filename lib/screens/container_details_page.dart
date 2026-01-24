import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:provider/provider.dart';
import 'package:gosh_distrobox_manager/src/rust/backends/distrobox/distrobox.dart';
import 'package:gosh_distrobox_manager/screens/container_terminal_page.dart';
import 'package:gosh_distrobox_manager/screens/apps_page.dart';
import 'package:gosh_distrobox_manager/providers/app_state.dart';

class ContainerDetailsPage extends StatefulWidget {
  final ContainerInfo container;

  const ContainerDetailsPage({super.key, required this.container});

  @override
  State<ContainerDetailsPage> createState() => _ContainerDetailsPageState();
}

class _ContainerDetailsPageState extends State<ContainerDetailsPage> {
  bool _isUpgrading = false;
  String? _upgradeTaskId;

  ContainerInfo get container => widget.container;

  bool get isRunning => container.status is Status_Up;

  Color get statusColor => switch (container.status) {
        Status_Up() => Colors.green,
        Status_Created() => Colors.blue,
        Status_Exited() => Colors.orange,
        Status_Other() => Colors.grey,
      };

  String get statusText => switch (container.status) {
        Status_Up(field0: final s) => 'Running${s.isNotEmpty ? ' $s' : ''}',
        Status_Created(field0: final s) =>
          'Created${s.isNotEmpty ? ' $s' : ''}',
        Status_Exited(field0: final s) => 'Exited${s.isNotEmpty ? ' $s' : ''}',
        Status_Other(field0: final s) => s.isNotEmpty ? s : 'Unknown',
      };

  String _extractDistroName(String image) {
    // Extract distro name from image URL
    final parts = image.split('/');
    final lastPart = parts.last.split(':').first;
    // Capitalize first letter
    if (lastPart.isEmpty) return 'Container';
    return lastPart[0].toUpperCase() + lastPart.substring(1);
  }

  IconData _getDistroIcon(String image) {
    final imageLower = image.toLowerCase();
    if (imageLower.contains('ubuntu')) return Icons.circle;
    if (imageLower.contains('fedora')) return Icons.filter_vintage;
    if (imageLower.contains('arch')) return Icons.architecture;
    if (imageLower.contains('debian')) return Icons.donut_large;
    if (imageLower.contains('alpine')) return Icons.landscape;
    if (imageLower.contains('centos') || imageLower.contains('rocky')) {
      return Icons.shield;
    }
    if (imageLower.contains('opensuse') || imageLower.contains('suse')) {
      return Icons.pets;
    }
    return Icons.dns;
  }

  void _copyToClipboard(String text) {
    Clipboard.setData(ClipboardData(text: text));
    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(
        content: Text('Copied to clipboard'),
        duration: Duration(seconds: 1),
      ),
    );
  }

  Future<void> _stopContainer() async {
    final appState = context.read<AppStateProvider>();
    final success = await appState.stopContainer(container.name);
    if (success && mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Container stopped')),
      );
      Navigator.of(context).pop(); // Return to container list
    }
  }

  Future<void> _upgradeContainer() async {
    setState(() => _isUpgrading = true);
    final appState = context.read<AppStateProvider>();
    final taskId = await appState.upgradeContainer(container.name);
    if (taskId != null && mounted) {
      setState(() => _upgradeTaskId = taskId);
      _showUpgradeProgressDialog(taskId);
    } else {
      setState(() => _isUpgrading = false);
    }
  }

  void _showUpgradeProgressDialog(String taskId) {
    showDialog(
      context: context,
      barrierDismissible: false,
      builder: (context) => _UpgradeProgressDialog(
        taskId: taskId,
        containerName: container.name,
        onComplete: () {
          setState(() {
            _isUpgrading = false;
            _upgradeTaskId = null;
          });
        },
      ),
    );
  }

  void _showCloneDialog() {
    showDialog(
      context: context,
      builder: (context) => _CloneContainerDialog(
        sourceContainer: container,
      ),
    );
  }

  void _confirmDelete() {
    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Delete Container'),
        content: Text(
          'Are you sure you want to delete "${container.name}"?\n\nThis action cannot be undone and all container data will be lost.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          FilledButton(
            style: FilledButton.styleFrom(backgroundColor: Colors.red),
            onPressed: () async {
              Navigator.pop(context);
              final appState = context.read<AppStateProvider>();
              final success = await appState.removeContainer(container.name);
              if (success && mounted) {
                Navigator.of(context).pop(); // Return to container list
              }
            },
            child: const Text('Delete'),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final distroName = container.name.isNotEmpty
        ? container.name
        : _extractDistroName(container.image);

    return Scaffold(
      appBar: AppBar(
        title: Text(distroName),
        actions: [
          if (isRunning)
            IconButton(
              icon: const Icon(Icons.terminal),
              tooltip: 'Open Terminal',
              onPressed: () {
                Navigator.of(context).push(
                  MaterialPageRoute(
                    builder: (_) => ContainerTerminalPage(container: container),
                  ),
                );
              },
            ),
          IconButton(
            icon: const Icon(Icons.refresh),
            tooltip: 'Refresh',
            onPressed: () => context.read<AppStateProvider>().refresh(),
          ),
        ],
      ),
      body: SingleChildScrollView(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // Hero Card
            Card(
              elevation: 0,
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(20),
                side: BorderSide(color: Theme.of(context).dividerColor),
              ),
              child: Padding(
                padding: const EdgeInsets.all(24),
                child: Column(
                  children: [
                    // Icon
                    Container(
                      width: 80,
                      height: 80,
                      decoration: BoxDecoration(
                        color:
                            Theme.of(context).colorScheme.surfaceContainerHighest,
                        borderRadius: BorderRadius.circular(20),
                      ),
                      child: Icon(
                        _getDistroIcon(container.image),
                        size: 40,
                        color: Theme.of(context).colorScheme.primary,
                      ),
                    ),
                    const SizedBox(height: 16),
                    // Name
                    Text(
                      distroName,
                      style: const TextStyle(
                        fontSize: 24,
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                    const SizedBox(height: 8),
                    // Image URL with copy button
                    InkWell(
                      borderRadius: BorderRadius.circular(8),
                      onTap: () => _copyToClipboard(container.image),
                      child: Container(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 12,
                          vertical: 6,
                        ),
                        decoration: BoxDecoration(
                          color: Theme.of(context)
                              .colorScheme
                              .surfaceContainerHighest,
                          borderRadius: BorderRadius.circular(8),
                        ),
                        child: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            Flexible(
                              child: Text(
                                container.image,
                                style: TextStyle(
                                  fontSize: 12,
                                  color: Theme.of(context).hintColor,
                                ),
                                overflow: TextOverflow.ellipsis,
                              ),
                            ),
                            const SizedBox(width: 8),
                            Icon(
                              Icons.content_copy,
                              size: 14,
                              color: Theme.of(context).hintColor,
                            ),
                          ],
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),

            const SizedBox(height: 24),

            // Status Section
            _buildSectionHeader('Container Status'),
            const SizedBox(height: 12),
            Card(
              elevation: 0,
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(16),
                side: BorderSide(color: Theme.of(context).dividerColor),
              ),
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: Row(
                  children: [
                    Container(
                      padding: const EdgeInsets.all(12),
                      decoration: BoxDecoration(
                        color: statusColor.withOpacity(0.1),
                        borderRadius: BorderRadius.circular(12),
                      ),
                      child: Icon(
                        isRunning ? Icons.play_circle : Icons.pause_circle,
                        color: statusColor,
                      ),
                    ),
                    const SizedBox(width: 16),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Row(
                            children: [
                              Container(
                                width: 10,
                                height: 10,
                                decoration: BoxDecoration(
                                  color: statusColor,
                                  shape: BoxShape.circle,
                                ),
                              ),
                              const SizedBox(width: 8),
                              Text(
                                statusText,
                                style: const TextStyle(
                                  fontWeight: FontWeight.bold,
                                  fontSize: 16,
                                ),
                              ),
                            ],
                          ),
                          const SizedBox(height: 4),
                          Text(
                            'ID: ${container.id}',
                            style: TextStyle(
                              fontSize: 12,
                              color: Theme.of(context).hintColor,
                            ),
                          ),
                        ],
                      ),
                    ),
                    if (isRunning)
                      FilledButton.tonalIcon(
                        onPressed: _stopContainer,
                        icon: const Icon(Icons.stop, size: 18),
                        label: const Text('Stop'),
                      ),
                  ],
                ),
              ),
            ),

            const SizedBox(height: 24),

            // Quick Actions Section
            _buildSectionHeader('Quick Actions'),
            const SizedBox(height: 12),
            Card(
              elevation: 0,
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(16),
                side: BorderSide(color: Theme.of(context).dividerColor),
              ),
              child: Column(
                children: [
                  _buildActionTile(
                    icon: Icons.system_update_alt,
                    iconColor: Theme.of(context).colorScheme.primary,
                    title: 'Upgrade Container',
                    subtitle: 'Update all packages',
                    onTap: _isUpgrading ? null : _upgradeContainer,
                    trailing: _isUpgrading
                        ? const SizedBox(
                            width: 20,
                            height: 20,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          )
                        : null,
                  ),
                  const Divider(height: 1),
                  _buildActionTile(
                    icon: Icons.apps,
                    iconColor: Colors.purple,
                    title: 'Applications',
                    subtitle: 'Manage exportable apps',
                    onTap: () {
                      Navigator.of(context).push(
                        MaterialPageRoute(
                          builder: (_) => AppsPage(container: container),
                        ),
                      );
                    },
                  ),
                  const Divider(height: 1),
                  _buildActionTile(
                    icon: Icons.content_copy,
                    iconColor: Colors.blue,
                    title: 'Clone Container',
                    subtitle: 'Create a copy of this container',
                    onTap: _showCloneDialog,
                  ),
                  const Divider(height: 1),
                  _buildActionTile(
                    icon: Icons.terminal,
                    iconColor: Colors.teal,
                    title: 'Open Terminal',
                    subtitle: 'Access container shell',
                    enabled: isRunning,
                    onTap: isRunning
                        ? () {
                            Navigator.of(context).push(
                              MaterialPageRoute(
                                builder: (_) =>
                                    ContainerTerminalPage(container: container),
                              ),
                            );
                          }
                        : null,
                  ),
                ],
              ),
            ),

            const SizedBox(height: 24),

            // Danger Zone
            _buildSectionHeader('Danger Zone', color: Colors.red),
            const SizedBox(height: 12),
            Card(
              elevation: 0,
              color: Colors.red.withOpacity(0.05),
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(16),
                side: BorderSide(color: Colors.red.withOpacity(0.2)),
              ),
              child: _buildActionTile(
                icon: Icons.delete_forever,
                iconColor: Colors.red,
                title: 'Delete Container',
                subtitle: 'Permanently remove container and data',
                titleColor: Colors.red,
                onTap: _confirmDelete,
              ),
            ),

            const SizedBox(height: 32),
          ],
        ),
      ),
    );
  }

  Widget _buildSectionHeader(String title, {Color? color}) {
    return Text(
      title.toUpperCase(),
      style: TextStyle(
        fontSize: 12,
        fontWeight: FontWeight.bold,
        color: color ?? Colors.grey,
        letterSpacing: 1.2,
      ),
    );
  }

  Widget _buildActionTile({
    required IconData icon,
    required Color iconColor,
    required String title,
    required String subtitle,
    VoidCallback? onTap,
    bool enabled = true,
    Widget? trailing,
    Color? titleColor,
  }) {
    return ListTile(
      contentPadding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
      enabled: enabled,
      leading: Container(
        padding: const EdgeInsets.all(10),
        decoration: BoxDecoration(
          color: enabled ? iconColor.withOpacity(0.1) : Colors.grey.withOpacity(0.1),
          borderRadius: BorderRadius.circular(12),
        ),
        child: Icon(
          icon,
          color: enabled ? iconColor : Colors.grey,
        ),
      ),
      title: Text(
        title,
        style: TextStyle(
          fontWeight: FontWeight.bold,
          fontSize: 14,
          color: enabled ? titleColor : Colors.grey,
        ),
      ),
      subtitle: Text(
        subtitle,
        style: TextStyle(
          fontSize: 12,
          color: enabled ? Theme.of(context).hintColor : Colors.grey,
        ),
      ),
      trailing: trailing ?? const Icon(Icons.arrow_forward_ios, size: 14, color: Colors.grey),
      onTap: onTap,
    );
  }
}

// Dialog for showing upgrade progress
class _UpgradeProgressDialog extends StatefulWidget {
  final String taskId;
  final String containerName;
  final VoidCallback onComplete;

  const _UpgradeProgressDialog({
    required this.taskId,
    required this.containerName,
    required this.onComplete,
  });

  @override
  State<_UpgradeProgressDialog> createState() => _UpgradeProgressDialogState();
}

class _UpgradeProgressDialogState extends State<_UpgradeProgressDialog> {
  final ScrollController _scrollController = ScrollController();

  @override
  void dispose() {
    _scrollController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Consumer<AppStateProvider>(
      builder: (context, appState, child) {
        final task = appState.getTask(widget.taskId);
        final output = task?.output ?? [];
        final isComplete = output.isNotEmpty &&
            (output.last.contains('completed') || output.last.contains('failed'));

        // Auto-scroll to bottom
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (_scrollController.hasClients) {
            _scrollController.animateTo(
              _scrollController.position.maxScrollExtent,
              duration: const Duration(milliseconds: 100),
              curve: Curves.easeOut,
            );
          }
        });

        return AlertDialog(
          title: Text('Upgrading ${widget.containerName}'),
          content: SizedBox(
            width: 400,
            height: 300,
            child: Container(
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
                      fontSize: 12,
                      color: Colors.green,
                    ),
                  );
                },
              ),
            ),
          ),
          actions: [
            if (!isComplete)
              TextButton(
                onPressed: () async {
                  await appState.cancelTask(widget.taskId);
                  widget.onComplete();
                  if (context.mounted) Navigator.pop(context);
                },
                child: const Text('Cancel'),
              ),
            if (isComplete)
              FilledButton(
                onPressed: () {
                  widget.onComplete();
                  Navigator.pop(context);
                },
                child: const Text('Done'),
              ),
          ],
        );
      },
    );
  }
}

// Dialog for cloning a container
class _CloneContainerDialog extends StatefulWidget {
  final ContainerInfo sourceContainer;

  const _CloneContainerDialog({required this.sourceContainer});

  @override
  State<_CloneContainerDialog> createState() => _CloneContainerDialogState();
}

class _CloneContainerDialogState extends State<_CloneContainerDialog> {
  final _nameController = TextEditingController();
  final _homePathController = TextEditingController();
  bool _isCloning = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    _nameController.text = '${widget.sourceContainer.name}-clone';
  }

  @override
  void dispose() {
    _nameController.dispose();
    _homePathController.dispose();
    super.dispose();
  }

  Future<void> _clone() async {
    if (_nameController.text.isEmpty) {
      setState(() => _error = 'Please enter a name for the new container');
      return;
    }

    setState(() {
      _isCloning = true;
      _error = null;
    });

    try {
      final args = CreateArgs(
        name: CreateArgName(field0: _nameController.text),
        image: '',
        init: false,
        nvidia: false,
        homePath: _homePathController.text.isNotEmpty ? _homePathController.text : null,
        volumes: [],
      );

      final appState = context.read<AppStateProvider>();
      final taskId =
          await appState.cloneContainer(widget.sourceContainer.name, args);

      if (taskId != null && mounted) {
        Navigator.pop(context);
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('Cloning ${widget.sourceContainer.name}...'),
          ),
        );
      }
    } catch (e) {
      setState(() => _error = e.toString());
    } finally {
      if (mounted) {
        setState(() => _isCloning = false);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Clone Container'),
      content: SizedBox(
        width: 400,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Create a copy of "${widget.sourceContainer.name}"',
              style: TextStyle(color: Theme.of(context).hintColor),
            ),
            const SizedBox(height: 16),
            TextField(
              controller: _nameController,
              decoration: const InputDecoration(
                labelText: 'New Container Name',
                border: OutlineInputBorder(),
              ),
            ),
            const SizedBox(height: 16),
            TextField(
              controller: _homePathController,
              decoration: const InputDecoration(
                labelText: 'Home Directory (optional)',
                hintText: '/home/user/containers/...',
                border: OutlineInputBorder(),
              ),
            ),
            if (_error != null) ...[
              const SizedBox(height: 16),
              Text(
                _error!,
                style: const TextStyle(color: Colors.red),
              ),
            ],
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: _isCloning ? null : () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(
          onPressed: _isCloning ? null : _clone,
          child: _isCloning
              ? const SizedBox(
                  width: 20,
                  height: 20,
                  child: CircularProgressIndicator(
                    strokeWidth: 2,
                    color: Colors.white,
                  ),
                )
              : const Text('Clone'),
        ),
      ],
    );
  }
}
