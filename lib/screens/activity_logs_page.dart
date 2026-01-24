import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:gosh_distrobox_manager/providers/app_state.dart';
import 'package:intl/intl.dart';

class ActivityLogsPage extends StatefulWidget {
  const ActivityLogsPage({super.key});

  @override
  State<ActivityLogsPage> createState() => _ActivityLogsPageState();
}

class _ActivityLogsPageState extends State<ActivityLogsPage> {
  String _selectedFilter = 'all';
  final TextEditingController _searchController = TextEditingController();
  String _searchQuery = '';

  @override
  void dispose() {
    _searchController.dispose();
    super.dispose();
  }

  List<TaskInfo> _filterTasks(List<TaskInfo> tasks) {
    var filtered = tasks.toList();

    // Apply search filter
    if (_searchQuery.isNotEmpty) {
      final query = _searchQuery.toLowerCase();
      filtered = filtered.where((task) {
        return task.description.toLowerCase().contains(query) ||
            task.output.any((line) => line.toLowerCase().contains(query));
      }).toList();
    }

    // Apply status filter
    if (_selectedFilter == 'success') {
      filtered = filtered.where((task) {
        return task.output.any((line) =>
            line.contains('completed') || line.contains('successfully'));
      }).toList();
    } else if (_selectedFilter == 'errors') {
      filtered = filtered.where((task) {
        return task.output.any(
            (line) => line.contains('failed') || line.contains('Error'));
      }).toList();
    } else if (_selectedFilter == 'running') {
      filtered = filtered.where((task) {
        return !task.output.any((line) =>
            line.contains('completed') ||
            line.contains('failed') ||
            line.contains('Error'));
      }).toList();
    }

    // Sort by start time (newest first)
    filtered.sort((a, b) => b.startTime.compareTo(a.startTime));

    return filtered;
  }

  String _formatTime(DateTime time) {
    final now = DateTime.now();
    final diff = now.difference(time);

    if (diff.inMinutes < 1) {
      return 'Just now';
    } else if (diff.inMinutes < 60) {
      return '${diff.inMinutes}m ago';
    } else if (diff.inHours < 24) {
      return '${diff.inHours}h ago';
    } else if (diff.inDays < 7) {
      return '${diff.inDays}d ago';
    } else {
      return DateFormat('MMM d, yyyy').format(time);
    }
  }

  IconData _getTaskIcon(TaskInfo task) {
    final desc = task.description.toLowerCase();
    if (desc.contains('create')) return Icons.add_circle;
    if (desc.contains('upgrade')) return Icons.upgrade;
    if (desc.contains('clone')) return Icons.content_copy;
    if (desc.contains('remove') || desc.contains('delete')) return Icons.delete;
    if (desc.contains('stop')) return Icons.stop;
    if (desc.contains('export')) return Icons.launch;
    return Icons.terminal;
  }

  Color _getTaskColor(TaskInfo task) {
    final hasError = task.output.any(
        (line) => line.contains('failed') || line.contains('Error'));
    final isComplete = task.output.any((line) =>
        line.contains('completed') || line.contains('successfully'));

    if (hasError) return Colors.red;
    if (isComplete) return Colors.green;
    return Colors.blue;
  }

  String _getTaskStatus(TaskInfo task) {
    final hasError = task.output.any(
        (line) => line.contains('failed') || line.contains('Error'));
    final isComplete = task.output.any((line) =>
        line.contains('completed') || line.contains('successfully'));

    if (hasError) return 'Failed';
    if (isComplete) return 'Completed';
    return 'In Progress';
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Activity Logs'),
        actions: [
          IconButton(
            icon: const Icon(Icons.refresh),
            tooltip: 'Refresh',
            onPressed: () => setState(() {}),
          ),
          IconButton(
            icon: const Icon(Icons.delete_sweep),
            tooltip: 'Clear completed',
            onPressed: () {
              context.read<AppStateProvider>().clearCompletedTasks();
              setState(() {});
            },
          ),
        ],
      ),
      body: Consumer<AppStateProvider>(
        builder: (context, appState, child) {
          final allTasks = appState.activeTasks.values.toList();
          final filteredTasks = _filterTasks(allTasks);

          return Column(
            children: [
              // Search and Filters
              Padding(
                padding: const EdgeInsets.all(16.0),
                child: Column(
                  children: [
                    SearchBar(
                      controller: _searchController,
                      leading: const Icon(Icons.search),
                      hintText: 'Search logs...',
                      onChanged: (value) {
                        setState(() => _searchQuery = value);
                      },
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
                    ),
                    const SizedBox(height: 12),
                    SingleChildScrollView(
                      scrollDirection: Axis.horizontal,
                      child: Row(
                        children: [
                          _buildFilterChip('All', 'all'),
                          const SizedBox(width: 8),
                          _buildFilterChip('Running', 'running',
                              icon: Icons.sync, color: Colors.blue),
                          const SizedBox(width: 8),
                          _buildFilterChip('Success', 'success',
                              icon: Icons.check_circle, color: Colors.green),
                          const SizedBox(width: 8),
                          _buildFilterChip('Errors', 'errors',
                              icon: Icons.error, color: Colors.red),
                        ],
                      ),
                    ),
                  ],
                ),
              ),

              // Stats Bar
              Container(
                padding:
                    const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
                decoration: BoxDecoration(
                  color: Theme.of(context).colorScheme.surfaceContainerHighest,
                ),
                child: Row(
                  mainAxisAlignment: MainAxisAlignment.spaceAround,
                  children: [
                    _buildStatItem('Total', allTasks.length,
                        Theme.of(context).colorScheme.primary),
                    _buildStatItem(
                        'Running',
                        allTasks.where((t) => _getTaskStatus(t) == 'In Progress').length,
                        Colors.blue),
                    _buildStatItem(
                        'Completed',
                        allTasks.where((t) => _getTaskStatus(t) == 'Completed').length,
                        Colors.green),
                    _buildStatItem(
                        'Failed',
                        allTasks.where((t) => _getTaskStatus(t) == 'Failed').length,
                        Colors.red),
                  ],
                ),
              ),

              // Timeline
              Expanded(
                child: filteredTasks.isEmpty
                    ? Center(
                        child: Column(
                          mainAxisAlignment: MainAxisAlignment.center,
                          children: [
                            Icon(Icons.history,
                                size: 64, color: Theme.of(context).hintColor),
                            const SizedBox(height: 16),
                            Text(
                              allTasks.isEmpty
                                  ? 'No activity yet'
                                  : 'No matching activities',
                              style: TextStyle(
                                fontSize: 18,
                                fontWeight: FontWeight.bold,
                                color: Theme.of(context).hintColor,
                              ),
                            ),
                            const SizedBox(height: 8),
                            Text(
                              allTasks.isEmpty
                                  ? 'Your container operations will appear here'
                                  : 'Try adjusting your search or filters',
                              style: TextStyle(color: Theme.of(context).hintColor),
                            ),
                          ],
                        ),
                      )
                    : ListView.builder(
                        padding: const EdgeInsets.symmetric(horizontal: 24),
                        itemCount: filteredTasks.length,
                        itemBuilder: (context, index) {
                          final task = filteredTasks[index];
                          return _buildTimelineItem(
                            context,
                            task,
                            isLast: index == filteredTasks.length - 1,
                          );
                        },
                      ),
              ),
            ],
          );
        },
      ),
    );
  }

  Widget _buildFilterChip(String label, String value,
      {IconData? icon, Color? color}) {
    final isSelected = _selectedFilter == value;
    return FilterChip(
      label: Text(label),
      selected: isSelected,
      onSelected: (selected) {
        setState(() {
          _selectedFilter = selected ? value : 'all';
        });
      },
      avatar: icon != null
          ? Icon(icon, size: 18, color: isSelected ? Colors.white : color)
          : null,
      selectedColor: color ?? Theme.of(context).colorScheme.primary,
      checkmarkColor: Colors.white,
    );
  }

  Widget _buildStatItem(String label, int count, Color color) {
    return Column(
      children: [
        Text(
          '$count',
          style: TextStyle(
            fontSize: 20,
            fontWeight: FontWeight.bold,
            color: color,
          ),
        ),
        Text(
          label,
          style: TextStyle(
            fontSize: 12,
            color: Theme.of(context).hintColor,
          ),
        ),
      ],
    );
  }

  Widget _buildTimelineItem(BuildContext context, TaskInfo task,
      {bool isLast = false}) {
    final color = _getTaskColor(task);
    final icon = _getTaskIcon(task);
    final status = _getTaskStatus(task);
    final isRunning = status == 'In Progress';

    return IntrinsicHeight(
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Column(
            children: [
              Container(
                width: 36,
                height: 36,
                decoration: BoxDecoration(
                  color: color.withOpacity(0.2),
                  shape: BoxShape.circle,
                  border: Border.all(color: color.withOpacity(0.3), width: 2),
                ),
                child: isRunning
                    ? Padding(
                        padding: const EdgeInsets.all(8),
                        child: CircularProgressIndicator(
                          strokeWidth: 2,
                          color: color,
                        ),
                      )
                    : Icon(icon, size: 20, color: color),
              ),
              if (!isLast)
                Expanded(
                  child: Container(
                    width: 2,
                    color: Theme.of(context).dividerColor,
                  ),
                ),
            ],
          ),
          const SizedBox(width: 16),
          Expanded(
            child: Padding(
              padding: const EdgeInsets.only(bottom: 24.0),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    mainAxisAlignment: MainAxisAlignment.spaceBetween,
                    children: [
                      Text(task.description,
                          style: const TextStyle(
                              fontWeight: FontWeight.bold, fontSize: 16)),
                      Text(_formatTime(task.startTime),
                          style: const TextStyle(
                              color: Colors.grey,
                              fontSize: 12,
                              fontWeight: FontWeight.w500)),
                    ],
                  ),
                  const SizedBox(height: 4),
                  Container(
                    padding:
                        const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
                    decoration: BoxDecoration(
                      color: color.withOpacity(0.1),
                      borderRadius: BorderRadius.circular(4),
                    ),
                    child: Text(
                      status.toUpperCase(),
                      style: TextStyle(
                        color: color,
                        fontSize: 10,
                        fontWeight: FontWeight.bold,
                      ),
                    ),
                  ),
                  if (task.output.isNotEmpty) ...[
                    const SizedBox(height: 8),
                    InkWell(
                      onTap: () => _showTaskOutput(context, task),
                      borderRadius: BorderRadius.circular(8),
                      child: Container(
                        padding: const EdgeInsets.all(8),
                        decoration: BoxDecoration(
                          color: Theme.of(context).brightness == Brightness.dark
                              ? Colors.grey[800]
                              : Colors.grey[100],
                          borderRadius: BorderRadius.circular(8),
                          border:
                              Border.all(color: Theme.of(context).dividerColor),
                        ),
                        child: Row(
                          children: [
                            Expanded(
                              child: Text(
                                task.output.last.length > 50
                                    ? '${task.output.last.substring(0, 50)}...'
                                    : task.output.last,
                                style: const TextStyle(
                                    fontFamily: 'monospace', fontSize: 12),
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                              ),
                            ),
                            const SizedBox(width: 8),
                            Icon(Icons.open_in_new,
                                size: 14, color: Theme.of(context).hintColor),
                          ],
                        ),
                      ),
                    ),
                  ],
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }

  void _showTaskOutput(BuildContext context, TaskInfo task) {
    showModalBottomSheet(
      context: context,
      isScrollControlled: true,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(20)),
      ),
      builder: (context) => DraggableScrollableSheet(
        initialChildSize: 0.7,
        minChildSize: 0.5,
        maxChildSize: 0.95,
        expand: false,
        builder: (context, scrollController) => Column(
          children: [
            Container(
              padding: const EdgeInsets.all(16),
              decoration: BoxDecoration(
                border: Border(
                    bottom: BorderSide(color: Theme.of(context).dividerColor)),
              ),
              child: Row(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  Text(task.description,
                      style: const TextStyle(
                          fontWeight: FontWeight.bold, fontSize: 18)),
                  IconButton(
                    icon: const Icon(Icons.close),
                    onPressed: () => Navigator.pop(context),
                  ),
                ],
              ),
            ),
            Expanded(
              child: Container(
                color: Colors.black87,
                child: ListView.builder(
                  controller: scrollController,
                  padding: const EdgeInsets.all(16),
                  itemCount: task.output.length,
                  itemBuilder: (context, index) {
                    final line = task.output[index];
                    Color textColor = Colors.green;
                    if (line.contains('Error') || line.contains('failed')) {
                      textColor = Colors.red;
                    } else if (line.contains('warning')) {
                      textColor = Colors.orange;
                    }
                    return Padding(
                      padding: const EdgeInsets.symmetric(vertical: 1),
                      child: Text(
                        line,
                        style: TextStyle(
                          fontFamily: 'monospace',
                          fontSize: 12,
                          color: textColor,
                        ),
                      ),
                    );
                  },
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
